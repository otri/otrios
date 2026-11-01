<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces map M1 qualification — 2026-11-01

RFC 0028 M1 is deployed at <https://otrios.otri.ca> and ready to try. The common
Rust map and settings run in browsers and on Frame. This records automated
physical-device acceptance. Aaron reported that the updated appearance looked
good; a separate physical finger-touch observation was not reported.

The operational contract is [Spaces](../SPACES.md). The earlier
[Spaces migration](spaces-frame-2026-11-01.md) records RFC 0027's foundation.
[Sanitized evidence](data/spaces-map-m1-2026-11-01/README.md) contains hashes,
counts and synthetic browser screenshots. Private Frame pixels and payloads
remain outside the repository.

## Delivered scope

The map filters Spaces by admission, hides empty Spaces outside Edit, and keeps
profile/sign-in upper-right and Edit/Done lower-right. A single-app Space opens
its app directly. Administrators can rename/recolor Spaces, reorder them by title,
resize their corners, reorder apps and set Space-local app names. Keyboard actions
use the same expected-generation operations. Collection IDs and epochs persist.

Shared vector artwork is restored in the map and Add App picker. Names are centered
beneath icons, including wrapped lines. The profile initial is centered horizontally
and vertically using its glyph ink bounds. App settings expose an implicit name
field and immutable bundle ID. Share App, Export Data and Replace Data truthfully
report unavailable. M2 retained-data presentation, M3 cross-Space moves and M4
portable transfers remain future milestones. Existing CRUD controls remain under
Space settings / Apps & Data; unmeasured active-user statistics report unavailable.

## Source, signatures and compatibility

| Role | Release | Runtime source |
| --- | --- | --- |
| Final active | `df40326c5dd7b2391beae584bb79a10f4c0bb7482b4a61e8b62468388ae0408b` | `e2a782a76f11d339c09281714bfd587b7b04d81b` |
| Exercised compatible fallback | `3a8da73e44410d7a3852e64f8cdd2068f33d59a84000bc7946ee1d9fb593298c` | `85a784592912a0608b515fba55bd3625a8d9fb6a` |

`python3 scripts/watch-dashboard.py --once --stage-only` froze the pushed source,
ran build checks, compiled ARM/native and browser hosts and signed all 12 installed
bundle closures with the Mac's existing publisher. Independent Mac and Frame
verification checked the complete signature/asset/bundle closure. All frozen
runtime inputs matched the recorded commit. Later acceptance/tooling commits
do not change deployed runtime code. Both browser suites used the **exact signed release Wasm**.
Existing strict SSH host pinning and publisher identity were retained.

State protocol **v17**, Spaces envelope **v2** (numeric management version 1),
registry **OTSP6** and opening release-control **v1** remain unchanged in this
refinement. The initial M1 OTSP5 → OTSP6 migration added only presentation defaults
and empty app labels. It preserved four Spaces, 14 collections, generation 22,
existing ownership/membership, policies, epochs, deletion decisions, pairing and
original-retention records. The newer Frame-owned Space was preserved too.
OTSP5-only binaries are not supported fallbacks after that migration.

## Preservation and rollback

Before final cutover, stopped services provided a fresh managed paired backup and
operational snapshot on Frame. The private Mac archive verified all
**261 inventory entries**, **84,746,240 archive bytes**.
Credentials and payloads stayed in private storage. Legacy originals were not
finalized or reclaimed.

The previous signed OTSP6 release and matching host helpers were preserved outside
the pruned release store. A code-only rollback exercised it, then reactivated the
final signed release from its independently verified preserved closure. Native
and a held anonymous browser reached both releases. Latest registry, decision
ledger, stable owner lock and storage pairing remained current. No old data
snapshot was restored over confirmed state.

Audits before rollback, on fallback and after reactivation preserved:

- All confirmed values, signals and root payloads in **12 documents**.
- All **269 preexisting writer receipts**;
  any extra registrations were unused, with sequence zero and empty digests.
- All **48 chat messages** and their digest; Counter stayed **4**.
- **37 other baseline state files**, byte for byte.
  One collector-owned generated Monitoring presentation changed after monitoring
  resumed. Its service binding was verified on Frame; it was retained.
- Auth configuration, Aaron's verifier, pairing, safety metadata and touch calibration.

These comparisons use the fresh cutover baseline and the existing private
post-reactivation snapshot. The final aggregate comparison did not restart services
or export private file-level metadata. The timer and collector resumed successfully.
Authority, native display and tunnel are active.

## Validation and physical acceptance

- `TMPDIR=/private/tmp bash scripts/check.sh` passed: strict Clippy, 209 core
  library tests, 25 Spaces metadata tests, 25 Spaces UI tests, 22 separate
  server/client process cases, workspace and bundle/tooling checks. The short
  temporary directory is needed by macOS local socket tests.
- [Linux/native/Wasm CI](https://github.com/otri/otrios/actions/runs/41299100540) passed for the final runtime commit.
- `bash scripts/build-dashboard-web.sh` passed. Both `scripts/qualify-spaces-browser.py`
  and its `scripts/test-spaces-map-browser.mjs` driver passed against exact signed
  Wasm: map/settings, Add App artwork, 1024/736/390/320-pixel bounds, pointer and
  keyboard ordering/resizing, mounted-peer preservation, scoped switching and
  anonymous filtering. Full regression covered retained data, public comments,
  ink/chat isolation, revocation, reviewed publication and verified attachment
  download independent of source deletion, plus anonymous socket-drop recovery.
- An earlier isolated physical ARM reconnect test passed at source `85a7845` under
  a 192 MiB cap with private network/tmp/devices and no live-state access. Earlier
  M1 ARM metadata/UI and separate-client tests are supporting historical evidence;
  final native acceptance below uses the deployed final code.
- `python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only`
  passed **171 checks**. Human login was not exercised; its verifier was preserved.
- A held anonymous browser reached fallback and final reactivation with all 48
  messages, public-only discovery, no composer and release-control v1. The restart
  probe queried discovery during state recovery and was not counted as a completed
  live reconnect test. Exact signed fixture socket-drop recovery passed; no further
  live restart was performed. A fresh root visit opened the map without Edit and
  launched Brass Relay from its Space title.
- Final native current/runtime/last-good hashes agreed, with confirmed state,
  ready stream, idle input and zero pending commands. Direct GLES2/KMS used physical
  800×1280 output rotated 90°, RGB565 (5/6/5/0), and one calibrated touch device.
  Synthetic evdev taps through that actual reader exercised map → Household list
  → Counter → map → Brass Relay → map, with **zero Counter writes**. Calibration
  SHA-256 remained `44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

Qualification found and fixed scoped Space handoff retaining its old socket and
anonymous chat reconnect failing to reselect its read-only collection. Selection
now waits for the fresh mount epoch; online status returns only after a valid
current view, without presence or write privileges. A preexisting document test
helper race was fixed by waiting for the whole prefetch window; production document
rendering was unchanged. The initial physical probe's Brass Relay Quit coordinates
were corrected; its profile dialog was dismissed without saving.

The reusable `scripts/check-frame-release.py` now checks matching native/runtime
release hashes, state readiness, health and services, and optionally resumes the
collector in one SSH round trip, without authority restarts, rollback or state
exports. It was syntax-checked, CLI-checked and exercised successfully against the
accepted live release. [The shorter iteration loop](../FRAME_HOSTING.md#short-iteration-loop)
batches visual feedback before freezing, runs independent checks in parallel and
reserves rollback rehearsals for storage/protocol/deployment changes.

These dashboard results do not establish Linux shared-memory IPC or later RFC 0028
milestones. No kernel, driver or display/touch calibration changes were made.
