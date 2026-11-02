<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0028 portable transfers — 2026-11-02

Status: implementation and local integration checks pass. **Not deployed or
qualified on physical Frame yet.** This is the continuing combined qualification
record for [RFC 0028](../rfcs/0028-spaces-map-sharing-and-portable-data.md), including
the previously deployed retained-data and scoped-move baseline. The remaining
signed artifact, migration/rollback and real deployed browser/Frame gates below
must pass before the RFC is complete.

The continuation branch is `feat/spaces-portable-transfers`. It preserves the
retained-data and admin/moves histories and includes current `main`. The deployed
authority and native reader were rechecked over pinned SSH: both still run
`50e8bc2e1ece14aa9feafc366df095ea475c93e991c2be29cf05d19db7a28a0c`,
with `otrios-canvas-sync-v19` and healthy authority status. No release, service
configuration or confirmed data was changed by the local qualification.

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

These are development artifacts from a dirty implementation checkout. They do
not substitute for qualification of the final frozen signed Wasm/native pair,
Safari acceptance or physical-device input through the Frame's calibrated reader.

## Remaining release gates

- Freeze committed implementation inputs and build/sign through the existing Mac
  workflow. Qualify the exact emitted browser/native bytes and populated map.
- Preserve a private paired pre-cutover backup and confirm the existing identities,
  layouts, memberships, bindings, app-owned content and assets before migration.
- Activate the completed signed code replica before publishing upgrade events;
  preserve opening control hello v1 while negotiating state v20/Spaces v5.
- Verify OTSP8-to-OTSP9 migration on the live environment without rebinding existing
  collections or losing confirmed content. A v19/OTSP8 writer is not a compatible
  rollback after this upgrade; keep and exercise a verified v20/OTSP9 fallback.
- Exercise real deployed browser and physical Frame export/replacement and signed
  app publication/import, including file/shared-HTTPS URL receipt, explicit review,
  current host operator policy and the Mac signer bridge. Preserve existing content
  through activation and rollback, clean up disposable acceptance scopes and record
  actual aggregate results without exporting private inventories or pixels.
- Finish with matching signed pointers, healthy authority/collector services,
  confirmed native state and idle input; then promote the validated source pin.

No PR has been opened. This record remains incomplete until those gates have
authoritative evidence. UI/process checks do not establish Linux shared-memory IPC.
