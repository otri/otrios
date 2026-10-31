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

## Shared-session discovery checkpoint

Clean source `9f94f26142c1280fa7e3b649704fa352072abebb` passed the full
`scripts/check.sh` suite, 14 direct Node session tests, and the rebuilt Chromium
probe with zero runtime exceptions. The [v10 report](data/spaces-discovery-2026-11-01.json)
records its exact artifacts. Added covered checks exercise metadata filtering,
the maximum-size snapshot, malformed management frames and lost-reply recovery
without management replay. A real shared-session scenario races initial capture
with attachment, observes subsequent changes, rejects Member management and
removes a revoked Member's closed-Space metadata. This qualifies the channel and
host adapters; shared Apps & Data controls and physical Frame remain pending.

## Current-policy activation checkpoint

Clean source `1401d2f5408d8538c5ceb12d2c435cc3a0cb3969` passed the full
`scripts/check.sh` suite and rebuilt Chromium qualification with zero runtime
exceptions. The [v11 report](data/spaces-activation-2026-11-01.json) identifies its
artifacts. Managed candidates now remain invisible until an authority Activated
reply. Covered tests hold protected candidate state/aggregate content offscreen,
discard denied/disconnected/cancelled candidates and preserve late reconnect
edits only within matching bindings. A real authority scenario revokes membership
between qualification and acceptance, receives denial rather than activation,
rejects the subsequent write and verifies unchanged data through another client.
These results still do not establish physical Frame acceptance.

## Owned-document collection checkpoint

Clean source `707d8e98777550c337e05ec6fbc5568c39e97191` passed the full
`scripts/check.sh` suite and rebuilt Chromium probe with zero runtime exceptions.
The [report](data/spaces-owned-documents-2026-11-01.json) records its artifacts.
Fifteen covered Space tests include the canonical collection schema; seven tests
in the real process suite include two confirmed legacy documents migrating into one app
collection, independent documents and Space isolation, restart without an explicit
registry flag, and incompatible-code rejection without checkpoint replacement.
Restoring compatible code reopens the original identities and values. Browser
qualification retains the existing separate-client admission checks; the
multi-document/migration scenario is authority process evidence. Frame and the
remaining lifecycle controls are still pending.

## Shared host controls and retained removal checkpoint

Clean source `fa29e9aa0b7778cd73d066ffb812b24e09e75350` passed the full
`scripts/check.sh` suite and rebuilt Chromium qualification with zero runtime
exceptions. The [report](data/spaces-host-controls-2026-11-01.json) identifies the
source, authority/Wasm bytes and signed disposable fixture. State protocol v11
and release-control v1 remain unchanged.

Sixteen covered Space tests, nine host UI tests and eight tests in the real
authority process suite pass (seven authority scenarios plus the child-client
entry test). The UI checks cover role-filtered controls, responsive bounds,
local-only forms, stale roster redaction, publisher/host action separation,
confirmation before Quit, reconnect denial and Home/discovery ordering. The
process scenarios prove removal revokes an active client, preserves Admin-visible
data and restores the same collection/value after restart and reattachment.
Configured collector principals cannot create Spaces or acquire human roles.
The ten native owner tests also pass, including held-frame revocation and the
Space-selection helper; these are adapter tests without physical Frame hardware.

The browser probe uses the shared host controls to Quit while another participant
continues editing, remove the app and revoke that participant, inspect retained
data, reattach the original collection/value, and switch to the isolated second
Space. Space A ends at 3 while Space B remains 0.

![Admin-visible retained app data](data/spaces-retained-data-2026-11-01.png)

An intermediate full run encountered a connection reset in the existing
release-bootstrap integration test. Its isolated rerun and the final complete
suite passed. Local uninstall, collection-owned auxiliary channels, durable
deletion, public sharing and physical Frame deployment remain pending.


## ARM emulation qualification

The clean `fa29e9aa0b7778cd73d066ffb812b24e09e75350` source archive builds the
ARMv7 host, authority, collector, session bootstrap and app packer using Rust
1.90.0 and the matching ARM C compiler/archiver. ELF inspection confirms ARM
machine code. The [report](data/spaces-arm-emulated-2026-11-01.json) records the
artifact hashes, compiler configuration and emulator version.

All eight top-level process tests pass against the ARM authority under QEMU:
seven authority scenarios and the child-client entry helper. They cover Space
isolation/admission, shared discovery, staged revocation, confirmed legacy-root
migration, collector separation and retained removal/reattachment. The later
264-document cache-pressure case was validated natively in `bf8e371`; it is not
part of this older ARM source qualification.

The builder initially selected a host C compiler despite its ARM Rust linker.
The corrected configuration explicitly selects target CC/AR; `72894de` also
tracks those settings in the build script and rejects an implicit host compiler
for a cross build. The modified script was separately run with the real ARM
compiler and its generated objects inspected. Its full required suite passes.

QEMU uses the container's installed armhf runtime libraries (`-L /`), rather than
the linker's cross sysroot. These unsigned binaries were never activated on
Frame. This evidence establishes emulated authority execution; physical display,
touch, signed activation and rollback still require Frame acceptance.

## Scoped ink and observation-fence checkpoint

Clean source `bc6f2ceae9f81a2d3cecfaaecd4b177b7854d8f8` passed the full
`scripts/check.sh` suite and rebuilt Chromium qualification with no runtime
exceptions. The [report](data/spaces-ink-2026-11-01.json) identifies its artifacts.
State protocol v12 scopes auxiliary payloads by accepted mount epoch and fences
discovery refreshes with echoed observation IDs; release-control v1 is unchanged.

The covered checks include 16 Space tests, ten host UI tests and two auxiliary
frame tests. Ten top-level process tests pass: nine authority scenarios plus the
child-client entry helper. The new ink scenario checks accepted versus staged
admission, isolation by Space/app/document, stale epochs, subscription release on
Quit, membership/attachment revocation, unaffected sibling use and fresh trails
after reattachment. The metadata ordering regression checks that a delayed old
snapshot cannot satisfy a new observation, repaint a roster or strand a removed
app. All 13 browser-worker tests pass and now run in the required suite, including
cookie revocation before reconnect and renderer-credit-independent redaction.

Real Chromium pointer strokes reach a second client in the same Space. Another
Space receives neither live points nor recent history after resubscribing.
Drawing continues after one participant quits. The same probe also repeats the
counter lifecycle and Space-picker checks.

![Scoped drawing in the disposable browser fixture](data/spaces-ink-2026-11-01.png)

This is local authority/browser evidence. The earlier ARM-emulation report names
its older source explicitly; physical Frame acceptance remains pending. Managed
chat/collection integration, local uninstall, durable deletion and public sharing
are still required before RFC completion.
