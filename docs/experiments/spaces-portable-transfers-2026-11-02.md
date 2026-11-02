<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0028 portable transfers — 2026-11-02

Status: **implemented and deployed; Safari acceptance remains pending**. This is
one continuing qualification campaign for the combined [RFC 0028](../rfcs/0028-spaces-map-sharing-and-portable-data.md)
scope. The [aggregate record](data/spaces-transfers-2026-11-02/acceptance.json)
contains technical results only. RFC completion and a final review PR remain
pending until Safari acceptance passes.

The implementation is pushed at `9a61137e8585531ab6930d61723d71f2fa7bbafb` on
`feat/spaces-portable-transfers`. It preserves the retained-data/admin-moves
histories and includes current `main`. Before cutover, authority and native ran
`50e8bc2e1ece14aa9feafc366df095ea475c93e991c2be29cf05d19db7a28a0c`
with state v19. The final active signed release is
`057aca96209224b3249ef9895a390c456e6a26cb466861e0552d3db41665223c`,
using state v20, Spaces envelope v5, registry OTSP9 and opening control v1.

## Implementation and local evidence

The [transfer contract](../TRANSFERS.md) records exact framing, limits, authorization
and fixed fixture bytes. The shared panels now connect to browser file/URL intake,
public code publication, complete confirmed exports and reviewed replacement.
Frame has an asynchronous private Transfers-folder adapter with opaque, metadata-bound
file selection, bounded streaming, atomic export completion and account cancellation.
Installation and replacement still commit only through the existing Spaces channel.

`TMPDIR=/private/tmp bash scripts/check.sh` exited **0** on the integrated candidate.
The final log is retained locally at `target/rfc0028-check-final.log`. Rust 1.90.0,
locked offline dependencies, isolated `CARGO_TARGET_DIR` and two test threads were
used. The gate includes strict workspace Clippy, Rust/Python/Node checks, signed
release installation/recovery and a separate real-authority process invocation.
The ordinary workspace invocation's process-test scaffolds are not counted as
process acceptance: the explicit `OTRIOS_SPACES_SERVER=...` invocation ran and
passed all **32** cases against the newly built server.

Relevant focused suites also pass:

- 242 core library cases, including portable fields/signals, complete chat history,
  durable import decisions, replacement recovery and OTSP8-to-OTSP9 migration.
- 38 shared host UI cases, including explicit import/replacement/publication
  confirmation, exact dataset targeting, cancelled/stale reviews and role loss.
- 18 native owner cases, including five transfer adapter cases covering private
  regular files, single-use picker IDs, changed files, quota, streaming round trips,
  malformed response cleanup and cancellation during stalled socket input.
- 14 transfer cases and four fixed portable data fixture cases. The independent
  Python oracle checks ZIP/checkpoint/causal framing and SHA-256 closure; Rust
  checks canonical FlexBuffers bytes, typed values, every reopened root, complete
  historical records and owned assets. This does not claim a second independent
  FlexBuffers implementation.
- Eight browser transfer adapter cases and 15 browser worker cases. A real browser
  exposed an incorrect `fetch` receiver that the original mocks did not catch;
  the adapter now binds it to the browser global and has a regression case.

The ARMv7 direct host/authority build passes with the pinned Docker toolchain.
Its source archive now includes `LICENSES`, required by the bundle tool, and omits
macOS extended attributes. The release Wasm build also passes. These platform
checks precede the final canonical signed build; final acceptance must identify
the exact signed artifact hashes. No dependencies were added in this continuation;
the native adapter enables the already locked randomness and CA-root dependencies.

## Isolated browser acceptance

The updated `qualify-spaces-browser.py` / `test-spaces-map-browser.mjs` run exited
**0** against disposable synthetic state. The driver waits for a stable rendered
epoch before clicking and uses actual browser keyboard input for pickup, rather
than assuming that an old accessible control still belongs to the current epoch.

The run verifies the common map, retained-data recovery/deletion, scoped movement
with explicit public exposure, and the actual portable data path:

1. Download a complete `.otdata` file through the browser host adapter.
2. Invoke the real file input and select that downloaded file for another Closed
   Space using the same installed app/schema.
3. Observe the review with its original target CID still active.
4. Confirm replacement, observe a fresh CID and verify that the exact outgoing
   CID is retained with `BeforeReplacement` provenance.
5. Reopen the imported app and verify the exported value, then remove only the
   disposable acceptance scope. Original fixture app CIDs remain unchanged.

The local records are `target/rfc0028-browser/{map,retained,moves,portable-data}-acceptance.json`,
with synthetic export/review/backup screenshots in that directory. The run reports
no runtime exceptions. This fixture's counter value is zero; nondefault values,
complete histories beyond the UI window and assets are established by the fixed
fixture and real-process tests, not inferred from that browser value.

The first development artifacts came from a dirty checkout. The full driver and
populated-map variant subsequently passed again using canonical Mac binaries and
the exact signed candidate Wasm (`17004c8f…`). Their synthetic catalog/release
remains separate from live production state. They do not establish Safari or
physical Frame input; the latter is covered by the deployed evidence below.

The thin `transfer_ui_probe` helper derives private on-device control coordinates
from negotiated discovery, the matching catalog and a local review recipe. It
performs no network I/O and rejects a recipe that queues management or changes
scope; a final confirmation is a coordinate query only. Export, replacement and
app-review recipes were checked against a disposable authority's real discovery,
and attempted import confirmation was rejected by the guard. Strict example
Clippy/build checks pass. These checks qualify coordinate generation only. Physical
input, actual host I/O and independently observed authority decisions must still
establish the on-device transfer results. The canonical ARM build includes this
helper and the existing authenticated management probe for that work.

## Signed deployment, migration and recovery

The existing Mac workflow built and verified candidate A:
`6fd416fb81ec6013eb493bf6575ea8729675ae867727ddecee0663c0a5cb141b`.
The exact Wasm SHA-256 is
`17004c8f0db653131b11f4bba522e750d5d6ac757291b318b5154096f5090f81`;
the native ARM SHA-256 is
`4095e8ed392a2d09a6ff1384986cb38fae0342e8ceb027e85a25ee8285378ea5`.
No implementation inputs changed after freezing. The exact-Wasm browser driver
and populated 14-app/four-Space variant both passed on committed source.

`backup-frame-release.py --tag 2026-11-02-spaces-portable-transfers` verified
265 files in a paired private Frame/Mac pre-cutover kit and restored services.
Raw metadata, inventories and credentials remain private. Before live activation,
the actual unprivileged ARM server recovered a disposable copy in a private
mount namespace at the original canonical storage path. This preserved pairing
invariants without weakening them. OTSP8-to-OTSP9 migration preserved metadata and
all 56 copied payload files; v20 reopened successfully. The obsolete writer
remained running but returned 503 for state/health with invalid storage. The check was corrected to assert this fail-closed behavior instead of
assuming process exit. The clone was removed and live state stayed unchanged.

Pinned-SSH deployment activated A before sending upgrade events. The live audit
then confirmed unchanged original Space metadata and 54 non-collector payload
files, device identity, credentials, calibration and collector binding. Primary
and latest registry use OTSP9. An anonymous browser tab opened on v19 upgraded
to A automatically, stayed connected, and exposed only public controls.

Aaron explicitly approved host-operator rights for the two existing human/Frame
identities after automatic approval review requested that specific authorization.
Actual IDs were resolved privately. No credentials or memberships were added or
changed. Only the configured operator list changed, with its original privately
backed up.

## Deployed transfer acceptance and compatible rollback

The physical Frame exercised these operations in one new Closed fixture Space:

1. Export through the shared UI and native adapter to a private 0600 `.otdata`
   file. Metadata generation remained unchanged.
2. Select that file through the actual native picker, observe review without a
   mutation, then explicitly confirm replacement. A fresh CID became active and
   the exact outgoing CID remained a BeforeReplacement backup. The complete
   1,235-byte archive re-export matched byte-for-byte. The first input
   sequence missed its target; resetting to a known home state and using paced
   input with readiness checks resolved the driver sequencing issue. Direct real-authority intake
   independently qualified the file and its temporary review was cancelled.
3. Receive a distinct standalone `transfer-counter` SDK payload. Review/cancel
   did not install code. The actual native file picker and explicit confirmation
   queued its durable import. `process-app-imports.py` used the existing Mac-only
   signer bridge to activate app-only release B:
   `d0fd9ecfb6327910171ebdde8f8b2c448959c6daea8a81fb1e67f370b65699b3`.
   Qualified host binaries and original data stayed unchanged.
4. Publish that verified SDK code through the native Share App / Publish App
   controls. An anonymous ordinary browser showed its title, Download App, the
   code-only explanation, project source and hosting guide. HTTPS download
   unpacked and verified against the independent pinned publisher with exact
   bundle digest `e54e2c9ab7bdbca46e9f32bafbb655391ca6c7ca9ea9e87393250a5108323365`.
   Actual authority HTTPS URL intake returned the same installed digest; its
   review was cancelled without a metadata change. This URL check used the real
   server fetcher; it is not a claim of typing the URL on the physical keyboard.

Input was synthetic evdev through the actual calibrated physical-device reader,
not observed human finger input. No Frame pixels or private inventories were
exported. Nondefault fields, long histories, historical profiles and owned assets
are established by fixed-fixture/process tests, not inferred from this small
live dataset. The production-origin host checker also passed all 180 public
signed-code, negotiation and isolation checks; production password login was
not tested.

Cloudflare returned error 1010 to the diagnostic's default Python User-Agent.
The ordinary browser and product URL fetcher succeeded. Giving the diagnostic
an identifying application User-Agent completed its independent download check;
no routing/security policy changed. See [Cloudflare's documented cause](https://developers.cloudflare.com/support/troubleshooting/http-status-codes/cloudflare-1xxx-errors/error-1010/).

A new replacement was confirmed under B, then verified code-only rollback B→A
and reactivation A→B both preserved its latest binding, exact metadata and all
60 non-collector payload files. Re-export stayed byte-identical in both releases.
No data snapshot was restored. A separately verified v20 fallback, matching
server and control tools remain in the private rollback kit independently of
cache pruning. The older v19 release is not a compatible current-data rollback.

The fixture Space was archived with its data removed through the authority's
normal flow; its three known Transfers files were removed. Removing only the
temporary SDK app through `install-app.py` produced final app-only release C:
`057aca96209224b3249ef9895a390c456e6a26cb466861e0552d3db41665223c`.
All A/B/C Wasm and native hashes are identical. The explicitly published SDK
code object remains independent of installed code and Space deletion; its exact
verified archive is the one accounted-for additional code object, with no data.

Final preservation rechecks confirm unchanged original metadata and relative
order, all 54 original non-collector files, credentials, memberships, identity,
calibration and collector binding. Only the added fixture appears archived.
C's full signed closure verifies; Mac/current/staging, authority and native
last-good agree. A cold native boot runs C itself, confirmed and idle, with
healthy authority, native and tunnel services and successful monitoring. The
existing Mac watcher/signing bridge is running on exact pushed source `9a61137…`
with its source inbox fingerprint matching the frozen build. Previous controls
and launch configuration remain private in the rollback kit.

## Remaining gate

Safari's computer-use entry point returned `cgWindowNotFound` after stalling;
no Safari acceptance is claimed. The user has been asked to open an accessible
Safari window. Complete that focused check, then finalize RFC status and the
review PR. No PR has been opened while this gate remains. Documentation-only
updates do not invalidate the frozen implementation checks. These UI/process
results do not establish Linux shared-memory IPC.
