<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0006: Durable Canvas Merkle state adapter

Status: deployed on browser and photo frame; [acceptance evidence](../experiments/canvas-merkle-adapter-validation.md) records migration, reconnect and policy checks. Extends RFC 0005 without changing primitive rendering or app UI v2.

## Ownership and conflict policy

Every admitted app State reference owns a separate immutable Merkle tree. Only
the authority assigns durable register dots. Clients submit bounded, schema-
checked absolute field assignments, never trees, causal clocks, tombstones or
receipts. The authority serializes accepted assignments in arrival order; for
concurrent assignments to the same field, the last accepted command wins. This
is an explicit authority policy over the generic causal-register library, not
multi-authority federation, a sequence/text CRDT or exactly-once event delivery.
Existing text fields remain atomic string registers. Touch movement/presence
remain ephemeral, and animation, focus/caret and rendering clocks remain local.

The `StateAdapter` keeps the received authority snapshot plus a RAM-only pending
map and at most one in-flight intent batch. New edits coalesce by field behind
that batch. Reads see the latest pending value, then in-flight value, then the
authority. Remote changes do not erase local pending edits. A reconnect prepares
a new lease, installs a complete authority snapshot and restores the RAM overlay.
Same-app reconnects keep the current signed UI editable and preserve focus/capture;
app replacement still freezes input while the new candidate is qualified.
Presentation and accessibility control values read the same pending overlay.
Process/browser termination can lose unacknowledged edits; code/font/blob caches
never act as document checkpoints.

## Durable receipts and replay

One random 128-bit nonce lives in each Client's RAM across app navigation and
socket reconnects. It is bound to the accepted server lease. The tree contains
only its SHA-256 fingerprint, not the nonce itself. This identifies intent
receipts; it is never a Monitoring access credential. Public/private admission
still comes from the signed catalog, exact Origin and existing server session.

Each accepted batch has an increasing sequence. The server atomically changes
both field values and that writer's receipt (sequence, command digest, server
time). A duplicate is acknowledged through the already-persisted state without
reassigning the field; a changed replay at the same sequence is rejected. A
lost acknowledgment therefore cannot replay an old slider/text assignment over
another device's newer state. All field permissions, kinds, bounds and duplicate
IDs are checked before any mutation, including on duplicate commands.

The client removes its in-flight overlay only when an authoritative snapshot
contains its receipt. It cannot mistake an older in-flight replication frame for
acknowledgment of a newer write. No second out-of-band optimistic ACK is sent.
A newer coalesced local edit remains visible while an older batch is confirmed.

Receipts are bounded to 1,024 slots per app, with a 24-hour resume window. An
expired slot can be reused; uncertain old in-flight batches are explicitly
rejected rather than silently replayed. A client with no in-flight intent may
start a fresh receipt. Exhaustion rejects admission; no causal metadata or
pending intent is silently truncated. Active writes refresh receipt time.
Server sessions and clients cache their slot, so ordinary writes/reconciliation
do not scan all writer receipts. Full admission/recovery does bounded scans.

## Tree and incremental validation

The immutable root register binds format, State reference, admitted schema and
server writer incarnation. Child kind 1 holds field registers, kind 2 durable
writer receipts, and kind 3 authority-owned five-minute signal windows. IDs are
bucketed into 32-way groups (`kind / slot div 32 / slot mod 32`), preserving the
library fanout bound for 64 fields and 1,024 receipts. Schema hashes are domain-
separated and bind scope/field or metadata kind/slot. All payloads use canonical
FlexBuffers, typed bounds and metadata-inclusive SHA-256.

The client verifies each transfer before replacing its snapshot. Subsequent
schema validation skips identical branch/group/leaf hashes and checks only new
records. Removing structure or changing immutable scope/schema metadata is
rejected. Metadata-only changes can advance replication without rebuilding the
scene when bound values are unchanged. This preserves current field selectors;
a general collection query/dependency engine remains outside this cutover.

Signal windows use authority receipt time, retain at most one final sample per
second for five minutes, and expire from the read projection without storage
writes. They do not reconstruct a full history or trust client timestamps.

## Commit and recovery

The host validates a cloned candidate and session, appends the changed Merkle
records to that app's journal, flushes them, and only then publishes the new
in-memory document/session and permits the resulting frame to be delivered.
Failure exposes no optimistic server root or successful receipt. A persistence
error retires further reads/writes of that room until restart because a failed
flush/rename can have an uncertain disk outcome.

Each `.merkle.log` record is `length u32 LE / SHA-256[32] / canonical root-chained
Merkle delta`. The delta binds its expected previous and next roots. The
`.merkle` checkpoint contains a full canonical snapshot. Journal bytes are capped
at 16 MiB; before appending beyond a 4 MiB checkpoint threshold, write/flush/
rename a checkpoint of the current committed root, flush its directory, then
truncate/flush the journal. Recovery recognizes that root inside an old journal,
so a crash between checkpoint rename and truncation does not lose commits or
replay the wrong base. A torn final record is truncated to the last complete
valid prefix. A complete bad checksum or root-chain gap fails closed.

Normal commits serialize only changed nodes, not the whole app tree. Directory
entries are flushed for checkpoint replacement and initial journal creation.
New files are mode 0600. This is one server/process per state directory; it does
not add multi-process writers, replicated durable consensus or storage repair.
The existing per-app 512 KiB materialized-state bound remains in force.

## Activation, policy and protocol

`otrios-canvas-sync-v2` carries versioned binary StateRequest/StateResponse
messages inside the existing epoch-scoped Canvas Sync envelope. Mount carries
the RAM writer nonce and any uncertain in-flight sequence. Initial admission and
receipt creation are durable before Mounted. The existing staged mount still
qualifies UI, code, schema and state before AcceptMount; the previous lease
remains active until that decision. Old epochs cannot mutate the new scope.

Read frames use the RFC 0005 one-flight subtree protocol with exact cursor/root
ACKs and explicit full resync. Reliable WebSockets do not repeatedly resend a
frame already emitted; latest roots coalesce while its ACK is pending. The
state-message ceiling is 192 per second to allow write intents plus read ACKs
when multiple devices edit, while peer/size/batch budgets remain bounded.

Monitoring remains protected before UI/code/state delivery, on every incoming
command, before outgoing private payloads, and on expiry/logout. Rebinding the
writer through a Sync/Hello is rejected for an accepted lease. A client never
controls an authority-only field, receipt, schema or root through its field IDs.

## Migration and acceptance

The signed release binds `otrios-canvas-merkle-state-v1` and sync v2. Existing
`.automerge` files are a trusted-file, one-time import of schema/scope-qualified
current values when no new checkpoint exists. They are retained for rollback;
legacy histories and old signal logs are not imported, and the network never
accepts Automerge bytes. Deployment must coordinate the new authority, browser
release, native executable and supervisor protocol checks.

Acceptance includes all existing Canvas/UI tests, RAM coalescing, lost-ACK
reconnect after a newer peer edit, unaccepted pending replay, duplicate/corrupt/
forbidden writes, receipt exhaustion/expiry, bounded signal windows, journal
recovery/checkpoint interruption and a real persistence failure followed by
restart and pending replay. Run real-process public/private policy tests and
verify browser plus one direct-rendered frame instance on the same signed
release. Report actual evidence separately from physical-finger acceptance.

Direct `serde` and `getrandom` dependencies reuse already-locked workspace
versions: canonical typed state envelopes and secure writer/epoch nonces are
now core responsibilities. No additional third-party package is introduced.
