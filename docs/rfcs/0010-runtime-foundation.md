<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0010: Practical Canvas runtime foundation

Status: implemented, qualified and deployed on
`feat/canvas-replication-pipeline`. This is the active dashboard milestone.
The user approved completing this foundation on 2026-10-19. Main and the
historical benchmark branch are not deployment targets for this work.
The [first foundation rollout](../experiments/canvas-foundation.md) is live;
The [packed transport/resource follow-up](../experiments/canvas-packed-wire.md) is
also live. The [final retirement and acceptance](../experiments/runtime-retirement.md)
closes this bounded milestone on 2026-10-20. Broader OS research remains separate.

## Selection rule

Choose changes that make the working system easier to maintain, more responsive,
or less expensive in CPU, memory and bandwidth. Structural simplification can
qualify through correctness, fewer owners and bounded behavior without a speed
win. Label optimization claims separately and support them with matched frame
measurements. Investigate repeatable regressions; do not hide them behind a
better compression ratio or fewer delivered input values.

Keep the Linux hardware base, safe shared Rust runtime, one authority process,
one negotiated session per client, server policy/durability and RAM-only client
intents. Do not expand this milestone into federation, arbitrary queries, a
custom allocator, production sandboxing or a sequence/text CRDT.

## Completion ledger

Source implementation, automated acceptance and live acceptance are separate
facts. An item is complete only when its applicable gates have evidence here or
in a linked experiment. Commit each independently reviewable increment.

| Work | Implemented foundation | Completion evidence |
| --- | --- | --- |
| One edit pipeline | Deployed: one pending map, one immutable issued batch, deadlines/final flush and bounded intent payloads | Passed runtime/browser/frame qualification; maintain these semantics through subsequent codec changes |
| Selective Merkle replication | Deployed sync v4: admitted UI coverage and writer-local receipts advance coherently | Passed fixed-field/subtree isolation, resync and policy gates; collection queries remain explicitly outside this milestone |
| Universal app boundary | All seven apps use `Canvas(uiRef, stateRef)` packages | Passed final ownership audit and browser/frame parity; obsolete global paths/imports removed; Monitoring owns its view templates |
| Presentation ownership | Animation, focus/caret, ink and GPU resources are local | Passed delayed-state/worker tests, staged animation/3D checks and exact signed frame GPU trial with 250 ms worker stalls; no document checkpoints or duplicate writable view state |
| Resource bounds | Intent payload/field limits, DispatchUsage, message/cache/journal limits and one-flight replication | Shared host diagnostics and [ownership/bounds audit](../ARCHITECTURE-RESOURCES.md) implemented; workspace checks, six native worker tests and 18 Node session/worker tests pass; live browser/frame diagnostics verified; no whole-process allocation claim |
| Canonical data and transport | Deployed sync v4: one bounded packed codec, canonical bytes unchanged; [RFC 0011](0011-packed-canvas-transport.md) | Passed malformed/oversize/version, 24 frame trials and browser/direct-frame rollout; normal replication stays raw after measured LZ4 costs; control hello remains v1 |
| Retirement and acceptance | Old renderer/global-room paths and unused Snapshot override removed; first browser/frame rollout passes | Passed final source/dependency audit, 12 matched frame replication trials, staged/live browser and direct frame; Automerge/SDL app paths retired, one renderer and authority retained |

## Edit and presentation semantics

Continuous inputs are replaceable, unissued absolute assignments. New samples
cannot postpone their deadline. A discrete assignment is immediately eligible;
it still waits for the single immutable issued write. Final release/cancel makes
the latest pending values eligible immediately. Reads use pending, issued, then
authoritative state. ACK, admission and resync bypass gesture throttling.

This is latest-value state, not guaranteed delivery of every button press or
intermediate text edit. A future counter/event/sequence operation needs its own
consolidation semantics before admission. Multi-field commands validate as a
whole and issue together; pending absolute assignments may supersede earlier
unissued values. Authority publication and durable receipt remain atomic.

The presentation owner consumes immutable selected values plus local transient
state. A network delay must not stop its clock or optimistic control feedback.
Closing a client may lose unconfirmed edits. Caches contain disposable immutable
resources and never become document checkpoints.

## Selective coverage and identity

Start with the existing bounded typed fields, not a general query language.
Coverage comes from the admitted UI: selected values, event/reducer inputs,
editor and motion declarations, requested signal windows, immutable scope/schema
metadata and that lease's own durable receipt. A client request is not a grant.
Multiple paths publish from one authority snapshot and advance as one coherent
view. Suppress publications whose covered hashes did not change, even when the
global app root changed. Missing/removed paths need explicit semantics and
resynchronization; never manufacture schema defaults for unknown live values.

Keep generic stable-key subtree paths useful outside Canvas. Collection members
use stable keys rather than array offsets when that operation family is added.
Membership/order watches and bounded projections are future incremental slices,
not a prerequisite for narrowing today's fixed-field replication.

## Qualification and rollout

1. Qualify the single queue and resource accounting without changing wire bytes.
2. Implement coherent covered replication and writer-local receipts; coordinate
   the state-protocol bump while keeping opening control v1 stable.
3. Audit app/host ownership and retire duplicate paths after behavior parity.
4. Compare a packed envelope over identical canonical nodes, then optional
   compression over identical uncompressed batches. Adopt the simpler adequate
   option; no compulsory schema generator or compression dependency.
5. Exercise browser and direct GLES frame on the same signed release. Verify
   navigation, input, final release, remote changes, reconnect/upgrade, private
   Monitoring and authority recovery. Record automated versus physical evidence.

### Implemented and qualified increments

The adapter exposes `DispatchUsage` and a 256 KiB combined pending/issued value
payload limit. Core tests cover all-or-nothing rejection, immutable-flight
charging, and ages preserved through coalescing/replay/restore. Canvas app tests
exercise immediate slider feedback under delayed confirmation, pointer Up and
Cancel flushes, and discrete controls/text bypassing continuous deadlines.
These are shared-runtime tests, not live browser or physical touch acceptance.

The generic Merkle library now supplies `Coverage` and
`Authority::subscribe_coverage`: at most 192 register/subtree paths, each within
the existing depth bound and authorized by trusted server roots. A register
selection includes its causal metadata but no unselected descendants. Addressing
ancestors carry no value unless selected separately. A full subtree shares its
existing immutable nodes. Missing targets remain watched.

One projected root/cursor publishes the entire covered view atomically, with the
existing one-flight ACK/resync/revocation rules. Filtered ancestors have new
view-local hashes; this is an authority-issued materialization, not a proof of
membership against the unfiltered root. A subscription caches one projection;
ACK and flight snapshots share those nodes. Fixed coverage requires a new lease
to change. No unbounded per-publication queue is introduced.

Library tests cover policy, canonical ordering, missing-target creation,
unrelated-change suppression, partial/corrupt-frame atomicity, slow readers and
resync. A real child process receives two paths coherently without sibling or
ancestor private payloads. The deployed Canvas integration uses
`StateSelection::from_ui`: shared selectors, handler/reducer
fields, editor/motion declarations and requested signal windows. The server
compiles it from admitted package bytes and fixes it before the first offer.
Only that lease's writer receipt is included. The client validates every required
field and returns an explicit error for unmaterialized fields rather than a
schema default. Authority documents/checkpoints/journals remain complete.

This cutover declares `otrios-canvas-sync-v3`; schema/node canonical bytes remain
v1 and opening release control remains v1. It is deployed with signed release
`ac0bbd21aa32155029f10df1fcdc56245ed4b9da6628a315124fd571b7ab062c`. The previous
v2 state client still receives release metadata through a v1 hello, with its
state stream blocked until upgrade. Publisher, browser catalog admission,
browser/native negotiation and authority manifest admission declare v3 together.
The unused `ServerMessage::Snapshot` override and its parallel map are removed;
read-only Monitoring continues using its separate replaceable `View` aggregate.

[Frame coverage measurements](../experiments/merkle-coverage.md) record 81
matched in-process trials. Covered views suppress unrelated deliveries and
reduce retained canonical bytes; projection adds authority work, especially with
few receipts. The subsequent [complete-client comparison and live qualification](../experiments/canvas-foundation.md)
record browser behavior, final convergence, retained server state and one direct
frame renderer. Neither report substitutes for physical touch/color acceptance.

Use fresh matched authorities for performance experiments. Report delivered
value age, final settlement, useful bytes, CPU, queue age and retained memory;
keep input rate, toolchain, UI and signed resources matched. A noisy three-run
difference is evidence to investigate rather than a claim of improvement.

Related: [dispatch measurements](../experiments/canvas-dispatch.md),
[replication pipeline](0007-bounded-replication-pipeline.md),
[selector semantics](0003-module-roots-and-state-selectors.md), and
[unified session](0009-unified-server-session.md).

### Completed retirement pass

The [final report](../experiments/runtime-retirement.md) records the deleted global
replica/intent overlay, old serializers/control branches, Automerge dependency
and startup import, SDL selection and obsolete frame service. Generic primitives,
static Monitoring aggregates and stable control compatibility remain deliberate
boundaries. A safe Flex structural preflight closes a demonstrated malformed
reader panic. Required checks, Linux CI, staged/live browser and one direct frame
renderer qualify the signed release `c549b460…`; canonical server state is retained.

Twelve matched frame trials preserve final convergence, delivery counts and
roughly the same latency/traffic. CPU medians rise modestly; the report investigates
added validation work without claiming isolated attribution. The authority binary
shrinks 28.75%, while guarded client binaries grow slightly. Exact signed GPU
trials maintain ~55 FPS under state-worker stalls, with missed vblanks disclosed.
This is a bounded ownership/correctness/maintenance milestone, not a universal
speedup or completed general-purpose OS. All applicable ledger gates above have
evidence; no required implementation work remains in this milestone.
