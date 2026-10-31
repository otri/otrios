<!-- SPDX-License-Identifier: Apache-2.0 -->
# Space admission qualification — 2026-11-01 JST

## Authority-bound chat profile preparation

The full `bash scripts/check.sh` suite passes for the authority-bound profile
preparation, including 24 covered chat tests and both real chat process scenarios.
Added tests preserve app-local profiles through restart and guest expiry, keep
authority identities and resume secrets out of room events, reject duplicate or
rebound saved actor mappings, and abandon revoked presence without writing an
unlinked journal. The session adapter still rejects managed chat; this is a
runtime prerequisite, not managed chat or physical Frame acceptance.

An initial full run failed because the chat restart helper connected to the
private listener after waiting only for the earlier public listener. The helper
now waits for both listeners with a bounded deadline and child-exit check. The
isolated recovery scenario and subsequent full suite pass.

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

## Collection-owned chat and first transcript checkpoint

Runtime source `1a169b07f5ad1898e4529d66f06a82dc5ef8ecac` passed the full
`bash scripts/check.sh` suite. Clean source
`a6f09f6dc130561364dbf6752945e93738f22ca6` additionally qualifies the browser
driver correction described below. Its rebuilt authority/Wasm fixture passes
Chromium acceptance with zero runtime exceptions; the
[report](data/spaces-chat-2026-11-01.json) records exact source, artifact hashes
and the signed disposable fixture. State protocol v13 adds the first transcript;
release-control v1 remains unchanged.

Twenty-five covered chat tests, 67 Canvas tests and eleven top-level Space process
tests pass (ten authority scenarios and the child-client helper). Added checks
qualify a bounded 64-message transcript without creating presence or writing a
journal. Its text is prepared incrementally before Accept, remains offscreen
until Activated, and is discarded if revoked while preparing. The first document
frame already contains readable glyphs before any active tick or Welcome. Tests
include a full Unicode/64-avatar window and header controls in landscape,
portrait and phone-sized views. Chat uses one host-owned Quit control without
covering Members or profile controls.

The real authority scenario isolates transcripts by Space, app and owning
document. Native access uses an explicit Frame identity; room commands cannot
choose an authority actor or resume a guest credential. Quit leaves another
participant sending, revoked sends are rejected, removal/restart/reattachment
recover the same collection, messages and app-local profiles, and a corrupt
replacement transcript leaves the previous activation usable. These are process
adapter tests, not physical Frame execution. All 37 direct Node assertions pass
across the worker, shared session and cookie-session checks.

The Chromium probe uses independent cookie contexts and the real Brass Relay UI.
Two clients share a transcript while another Space stays empty. A participant
continues sending after the other quits; reopening restores that participant's
profile. Removal clears the other client's transcript, and reattachment restores
the retained messages. The counter, discovery, revocation and ink checks also pass.

![Retained transcript after reattachment](data/spaces-chat-2026-11-01.png)

An initial clean probe cancelled its second synthetic ink stroke. Outgoing and
relayed samples matched; DOM tracing identified capture loss during a CDP drag
move that omitted `button: 'left'`. The driver now supplies both the pressed
button and bitmask and checks complete capture acquisition/release for both
strokes. Two consecutive runs and the clean run pass without temporary dispatch
hooks. The [input trace](data/spaces-chat-input-2026-11-01.json) records the clean
pointer/capture sequence. No ink runtime change was needed for this correction.

Explicit import of pre-Space chat history, device-local uninstall, durable
deletion and public sharing remain pending. This checkpoint makes no v13 ARM,
physical Frame or live-cutover claim.

## Explicit legacy chat migration checkpoint

Runtime source `c16f588bf4651119526c59e40e106808bf33de4e` passed the full
`bash scripts/check.sh` suite, including thirteen top-level Space process tests
(twelve authority scenarios plus the child helper). Two added migration scenarios
cover 70 confirmed legacy messages, older-window reads, source ownership across
processes, torn-tail recovery, independent fresh roots/Spaces, subsequent confirmed
writes, restart and no membership regrant. Missing/corrupt sources, conflicting
destinations and missing completed destinations fail without empty replacement.
Pending provisioning keeps admission unavailable after failure.

Chat import requires manifest version 2; v1 with an import declaration is rejected
before provisioning starts. A completed private import receipt validates the
current destination without replaying old data, including when the old source is
gone. Historical sender snapshots remain intact while an admitted Frame identity
gets its own profile.

Clean source `53ce0c5c12395f07e076ceaaca848a5caa52ea34` adds the migration browser
fixture. Its example passed targeted strict Clippy/build checks, and the fixture
tools passed syntax checks and real Chromium acceptance. The
[report](data/spaces-chat-migration-2026-11-01.json) records zero runtime exceptions,
exact artifacts and the signed disposable fixture. Both Members see the imported
legacy message in Space A; Space B receives none of that history. The probe also
repeats live sharing, Quit, removal and reattachment. Runtime/browser state
protocol v13 and release-control v1 are unchanged.

![Imported legacy message retained after reattachment](data/spaces-chat-migration-2026-11-01.png)

Permanent deletion and public sharing remain unfinished.
This is local process/browser migration evidence, not physical Frame acceptance.

## Device-local uninstall and chat revocation

Code checkpoint `ba4fe5f9919544ed1ee3b86966cc4fdce231826a` passed the full
`bash scripts/check.sh` suite (`/tmp/otrios-spaces-check-24.log`). The authority
binary SHA-256 was `b76c0d6622260b7c504c22d422b42b855407796d266c483d016842025cc69c82`.
State protocol v13 and release-control v1 are unchanged: installation metadata
and its notifications are device-local, not new shared-session messages.

- Fourteen covered host UI tests pass. Uninstall preserves a pending edit through
  confirmation, waits for captured release, cancels staged/accepted mounts, hides
  removed roots despite newer remote discovery, preserves the collection and
  signed catalog hash, leaves another participant usable, and reopens the same
  data after reinstall. Offline uninstall discards unconfirmed RAM intents and
  prevents them from resuming through reconnect or reinstall.
- A covered namespace test rejects invalid IDs, duplicate/unsorted roots and
  oversized root sets, removes child destinations, and keeps the trusted local
  Home available while invalid input closes app launches.
- Two native filesystem/IPC tests use independent Python writer processes and
  a real Unix socket. Concurrent writes preserve both removals, notifications
  resync the newest atomic snapshot, offline mutations survive observer restart,
  a second observer cannot steal the socket, and invalid file/socket types and
  permissions are rejected.
- The real native-worker test holds the display's only frame credit while an
  independent uninstall command runs. Its retained frame is revoked and redacted
  before replacement credit returns. These checks pass in the full thirteen-test
  native-owner suite; they do not use a physical display or touch device.
- Supervisor tests confirm that configured installation policy is passed as a
  required executable option, so an older binary cannot silently ignore it.

An earlier full run caught a managed-chat shutdown race: policy revocation could
end its auxiliary worker before the state worker delivered AccessRequired,
closing the shared socket early. The original real-process scenario reproduced
the failure on repetition 33. The adapter now distinguishes expected revocation
from failure of a still-authorized worker. The strengthened scenario verifies
that a revoked Member can refresh Home and that app removal still allows a new
authorized activation on the same socket. All 100 repetitions passed after the
fix, followed by the full suite, including all thirteen authority process tests.

This completes the S2 software increment. Permanent deletion, public sharing,
current ARM qualification and signed physical Frame acceptance remain outstanding.

## S3 decision-ledger foundation

Code checkpoint `5e7575a890e85c3602697bdd56dc79177e7592b4` passed the full
`bash scripts/check.sh` suite (`/tmp/otrios-spaces-check-25.log`). The authority
binary SHA-256 was `3f503859b70f2ab3c8875c26d78dbfd0136eee46facb43b6001ebef0a814d1a4`.
The private registry framing is now OTSP3; shared-session state protocol v13,
management v1 and release-control v1 are unchanged.

Eight covered store tests pass, including a separately invoked child helper.
Children exit without destructors at eight publication boundaries: before the
ledger, after its temporary-file sync/rename/directory sync, after those same
three primary-file steps, and before in-memory publication. Recovery preserves
the pre-decision binding before ledger replacement and applies the deletion
decision after it. Replacing the primary with an older snapshot cannot remove
the recovered tombstone or restore its collection identity. These are real
process-exit checks, not a simulation of power loss or storage hardware failure.

Additional cases verify exclusive ownership across atomic replacements,
missing-lock refusal, missing-primary reconstruction, missing-ledger refusal
even with an older OTSP2 backup, persistence poisoning and restart recovery,
partial initialization recovery, foreign/corrupt records, root pairing,
private permissions and symlink rejection. Ownership releases explicitly on
Drop so a concurrent process spawn cannot retain the lock through an inherited
descriptor while exec is still closing descriptors.

All sixteen existing registry/admission tests pass with legacy framing upgrades.
All fourteen authority process tests pass, including the new recovery scenario:
an older primary does not regrant a revoked Frame identity; the confirmed value
42 survives recovery; missing paired app storage and a missing ledger each keep
state unavailable while release hello still works; restoring the original
storage/ledger recovers 42 and keeps that identity revoked. Existing offline
document/chat migration and lifecycle checks also pass against this framing.

This is the S3 storage prerequisite. DeleteData remains unavailable through HTTP
and the shared session until producer shutdown, reclamation, backup/retention
workflows and cleanup reporting are integrated and qualified. No physical Frame
deployment or current ARM acceptance is claimed by these checks.

## S3 payload reclamation backend

Code checkpoint `e2af583678e0c4db6711ac265637d5dcd330b62b` passed the full
`bash scripts/check.sh` suite (`/tmp/otrios-spaces-check-26.log`). Six covered
reclamation tests pass, including a separately invoked child helper. The backend
requires a durable tombstone and producer quiescence. It rejects live collection
IDs and leaves all files intact while producers remain active.

Child processes exit at five cleanup boundaries. Restart retains the deletion
decision and finishes partial reclamation without changing another Space's
checkpoint, journal or presentation. Tests also reclaim chat/history/index and
migration-receipt files, interrupted replacement files, and more than 128 orphan
files through bounded pending/retry turns. Shared hard links and child symlinks
preserve the other collection's asset/history bytes; a symlink replacing the
collection directory is rejected. Reclamation is idempotent after completion.

This is backend evidence. Authority producer shutdown, cleanup reporting,
backup/retention and restore integration remain unfinished, so DeleteData is
still unavailable through management APIs. There is no physical Frame claim.

## S3 managed snapshots and fenced restore

Code checkpoint `4ce1dae437c95805a5eed88741fab532737c3d89` passed the full
`bash scripts/check.sh` suite (`/tmp/otrios-spaces-check-27.log`). The authority
binary SHA-256 was `4e2fe493ed927831f6d7b7c5f45ec9871b75bf16ab949627e0cc3a963815c31a`.
Private framing is now OTSP4 so earlier writers cannot ignore a pending restore.
State protocol v13, management v1 and release-control v1 are unchanged.

All 27 covered store tests pass, including their separately invoked child
helpers. Snapshot tests cover schema/history/asset retention, owner/control
exclusion, deleted collections, seven-day expiry, corrupt manifests, quotas,
symlink isolation and four real process-exit publication boundaries. Restore
tests cover six process-exit boundaries, corrupt staging that keeps admission
closed, safety snapshots, removal of later files, preservation of latest policy
and unrelated new collections, OTSP3 upgrades, and skipping deleted collections
whose old backup files have already been reclaimed.

All fifteen authority process tests pass. The new offline CLI scenario confirms
that a live authority excludes snapshotting; a confirmed value 42 can be backed
up, edited to 73 and restored to 42; and restoring payloads keeps the later
revocation of the separate Frame identity. A deliberately blocked replacement
leaves the restore marker durable and the restarted authority unavailable.
Repairing the target lets startup finish the plan before serving 42, without
restoring revoked membership.

This run also set `OTRIOS_SPACES_OLD_SERVER` to the preserved OTSP3 authority
whose SHA-256 is `dea4ca868239ecf209fefbbb80cd88c197687678284b8aac6fd48a41d4f0c352`.
It rejects the OTSP4 root and leaves primary/ledger bytes unchanged. That optional
older-binary portion is not exercised when the environment variable is absent.
The full run supplied it; this is a local process rollback check, not a signed
physical Frame rollout.

Snapshot/restore maintenance is available through the authority's offline
`--spaces-storage` actions. Automatic retention scheduling, legacy-original
retention/finalization, producer shutdown and management cleanup reporting remain
unfinished. DeleteData remains unavailable while those integrations are pending.


## Migration-original retention checkpoint

Source checkpoint: `eeccf4a5317e022cd710aedd5b6566c3758be1bd`.
The authority SHA-256 after the required check is
`37760056fc00c12780ae30ee2ccb24497f6a809963b74f8115c1098721d09167`.

`bash scripts/check.sh` passed with the preserved OTSP4 authority supplied as
`OTRIOS_SPACES_OLD_SERVER`. Its SHA-256 is
`4e2fe493ed927831f6d7b7c5f45ec9871b75bf16ab949627e0cc3a963815c31a`.
The real process test confirms that this older writer rejects OTSP5 without
changing the primary or latest ledger. Shared state protocol v13 and stable
release-control v1 are unchanged at this checkpoint.

Nine retention tests cover qualification before expiry, a fixed seven-day
finalization deadline, source fingerprint conflicts, active chat-owner exclusion,
missing originals, prevention of deleted-source reimport, older-primary recovery,
and bounded reclamation of interrupted checkpoint/chat-compaction files. The
retention inventory and finalization decision use the latest decision ledger;
losing a separate optional sidecar cannot reopen imports. The 36 storage tests
pass, including existing process-exit recovery boundaries. Lock ownership is
explicitly released on rejected recovery paths as well as successful Store drop.

All sixteen authority process cases pass. A new missing-checkpoint case verifies
that explicitly requesting a missing legacy original never creates an empty
replacement and leaves provisioning closed. Document and chat receipts recover
cleanup attribution without replaying confirmed data. The document case removes
the app from the installed catalog, reruns its completed historical manifest,
finalizes migration, and verifies that confirmed value 42 and the separate Frame
identity's later revocation survive. Existing full-history chat recovery remains
covered. Earlier intermediate validation was corrected before this checkpoint;
check 30 is the complete final run.

These are local software checks. Authority timer/status integration, connected
replica discard acknowledgements, deletion management/UI and S4 remain pending.
No physical Frame deployment or new browser acceptance is claimed here.
