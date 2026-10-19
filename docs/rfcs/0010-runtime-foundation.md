<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0010: Practical Canvas runtime foundation

Status: accepted direction, implementation in progress on
`feat/canvas-replication-pipeline`. This is the active dashboard milestone.
The user approved completing this foundation on 2026-10-19. Main and the
historical benchmark branch are not deployment targets for this work.

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

| Work | Existing foundation | Remaining gate |
| --- | --- | --- |
| One edit pipeline | Adapter owns one pending map and one immutable issued batch on the feature branch | App-level deadline/final-release/discrete-action/reconnect tests; explicit queue accounting; browser and frame qualification; remove prototype-only disposition after promotion |
| Selective Merkle replication | Generic authorized subtree leases; Canvas scopes by app | Derive bounded coverage from admitted selectors, handler reads and presentation; send only that writer's receipt; coherent publication of coverage; no unrelated-field traffic; gap/resync/revocation tests |
| Universal app boundary | All seven apps use `Canvas(uiRef, stateRef)` packages | Audit host/demo branches and dependencies, keep launcher opaque; remove obsolete routes/imports after parity; update stale ownership docs |
| Presentation ownership | Animation, focus/caret, ink and GPU resources are local | Delayed-state tests retain immediate local feedback and clock progression; no document checkpoints or duplicate writable view state |
| Resource bounds | Field/message/cache/journal limits and one-flight replication | Expose pending/flight bytes and age; enforce byte/count limits before admission; test slow peers, snapshot retention and overflow recovery; distinguish scratch counters from whole-process allocation claims |
| Canonical data and transport | Canonical Merkle hashing separate from binary Blob envelopes | One versioned bounded packed codec shared by browser/native; optional compression selected from matched costs; independent stable v1 control hello; malformed/oversize/unknown-version checks |
| Retirement and acceptance | Old renderer and most global-room paths removed | Explicit browser/frame matrix, one direct renderer, server persistence/policy/recovery tests; delete each redundant production path once replacements pass |

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

Use fresh matched authorities for performance experiments. Report delivered
value age, final settlement, useful bytes, CPU, queue age and retained memory;
keep input rate, toolchain, UI and signed resources matched. A noisy three-run
difference is evidence to investigate rather than a claim of improvement.

Related: [dispatch measurements](../experiments/canvas-dispatch.md),
[replication pipeline](0007-bounded-replication-pipeline.md),
[selector semantics](0003-module-roots-and-state-selectors.md), and
[unified session](0009-unified-server-session.md).
