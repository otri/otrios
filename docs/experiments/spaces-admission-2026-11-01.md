<!-- SPDX-License-Identifier: Apache-2.0 -->
# Space admission qualification — 2026-11-01 JST

Qualified source: `cf43975fdc73e13c2f8aa46d903e1be6220a08d9` on
`feat/spaces-and-app-data`, with a clean source tree. This is the authority and
browser admission increment of [RFC 0027](../rfcs/0027-spaces-and-app-data.md), not
completion of all four milestones or a Frame deployment. The [management guide](../SPACES.md)
records the current contracts, rollout procedure and unavailable lifecycle stages.

## Results

- Nine covered runtime tests pass for roles, multiple Admins, last-Admin protection,
  binding identity, retained collection identity, revocation/readmission, current
  installed-policy constraints and registry framing recovery.
- Real authority/child-client scenarios pass for independent Space values,
  unauthorized reads/writes, explicit Frame identity, active revocation/reconnect,
  Member management denial and concurrent last-Admin changes.
- Offline provisioning preserves a confirmed legacy value through journal replay,
  migration and restart. A completed manifest does not regrant a revoked member;
  a failed import leaves admission unavailable. Omitting the CLI flag does not
  reopen legacy access to a migrated state directory.
- A separately bound Monitoring collector can publish its collection's view;
  human Admins cannot publish it, and the collector receives no human membership.
  An independently attached Monitoring collection receives none of that view.
- The native owner test revokes a held frame before renderer credit returns for
  both private apps and ordinary apps used inside a Closed Space.

The repeatable Chromium probe signs in through the normal form using independent
cookie contexts. Two clients see Space A change from 0 to 1, while Space B stays 0.
Anonymous and authenticated non-Members receive denial. Removing the Member clears
that client's view; the remaining Admin continues editing Space A to 2. The probe
reports no runtime exceptions and preserves release-control v1.

![Disposable browser Space A after continued editing](data/spaces-admission-2026-11-01.png)

The [machine-readable report](data/spaces-admission-2026-11-01.json) records source,
authority/Wasm artifact digests and the signed browser fixture ID. The fixture
contains real browser and app bytes, with disposable signing keys and test data;
it contains no native executable and was never activated on Frame.

## Commands and limits

`bash scripts/check.sh` passed, including strict Clippy, workspace tests, real
process/session/bundle checks and Linux evdev fixtures. The native held-frame
case also passed directly through `cargo test -p otrios-dashboard --features
server,canvas,host-sync --test native_owner
app_transport_receives_remote_edits_reconnects_and_revokes_a_held_frame --locked
--offline`.

The browser build used Rust 1.90.0, the `wasm32-unknown-unknown` target and pinned
wasm-bindgen 0.2.129. After building the authority/example and web package from the
tested source, `python3 scripts/qualify-spaces-browser.py` passed against freshly
created disposable authorities and isolated Chromium contexts. The probe shuts
down those authorities and removes test credentials/signing keys afterward.

One intermediate full run failed the existing retained-document keyed-window
packing-count test. It passed in isolation and in subsequent full runs; no
retained-document code was changed for this increment.

This report establishes local authority/browser behavior and native-owner adapter
behavior. ARM execution, physical Frame presentation, signed live cutover and its
rollback remain unqualified here. No Linux shared-memory IPC claim is made.

## Collection-aware mount checkpoint

Clean source `9392387753f91f667b7e1a6ea2cb39e9513efbf5` passed the full
`scripts/check.sh` suite and the same rebuilt Chromium qualification with zero
runtime exceptions. The [v9 report](data/spaces-mount-binding-2026-11-01.json)
records the exact authority/Wasm digests and fixture release. Release-control v1
is unchanged; state protocol v9 carries the collection/action binding and first
aggregate view before candidate acceptance. Added runtime tests distinguish
same-collection pending-intent recovery from replacement-collection or read-only
admission, and prove read admission cannot allocate writer receipts or gain write
capability. This checkpoint also remains local qualification only.
