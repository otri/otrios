<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0033: Reclaim deleted OIB instances

Date: 2026-11-03. Status: **implemented, deployed and live qualified**.
See the [Frame/Wasm acceptance record](../experiments/oib-reclamation-2026-11-03.md).

The ten-allocation lifetime limit coupled durable identity to renderer capacity.
Repeated Add/Remove exhausted it while deleted descriptors, property registers,
presence and derived inspector references accumulated. Replace that lifecycle
with fresh monotonic object IDs and reusable renderer slots.

The authority stores resident allocation descriptors at `[4,4]`, each with a slot,
and one durable next-ID scalar at `[4,7]`. Dynamic property IDs derive from the
object ID plus the signed property rank, independently of slots. Renderer nodes,
controls and inspector locals derive from reusable slots. A surviving object's
slot remains stable. The existing six-live-instance, ten-resident-descriptor,
64-cell and scene/control/byte bounds remain; the signed factories reserve two
static slots, leaving eight reusable dynamic renderer slots in this demo. These
are simultaneous capacity limits rather than a lifetime creation budget.

After an admitted intent, the authority removes descriptors absent
from both draft and active graphs, then projects the current immutable root onto
the remaining metadata, property and presence paths. Removed causal registers
and empty ancestor branches disappear. Exact causal registers of survivors and
all app content/receipts remain. Invalid drafts retain objects still required by
the active graph. Normal bounded subscriptions/old pinned snapshots retain their
own revisions until released. Unused pending objects are also reclaimed while
an unrelated draft edit is invalid; the active graph remains unchanged.
Historical journal records remain until the
existing checkpoint rotation; this is not secure erasure or a general store GC.

Clients submit typed intents, never merge arbitrary prior OIB roots. The authority
ignores and acknowledges delayed property edits, removes and duplicates targeting
retired objects, without changing active UI or allocating new objects. The same
writer receipt/digest makes retry idempotent. A monotonic high-water mark identifies
retired dynamic IDs without a growing deleted-ID set. Unknown future IDs remain
invalid. Component-property CRDT joins and concurrent conflict semantics continue
unchanged. This reclamation rule relies on the single authority and must not be
reused as an unrestricted multi-authority CRDT merge rule.

Restore retains surviving authored objects and recreates missing signed factories
with fresh IDs and their declared bindings. Deleted instance overrides are gone;
new objects inherit the current component design. Reset additionally adopts the
signed compact defaults. Extras are reclaimed. Local inspector selectors must
reference only the selected live inspector; no deleted property lookup remains.
Gesture deferral still prevents reuse of displayed control slots mid-gesture.
Restore validates its disposable replacement roster before committing it. An
invalid replacement preserves the exact active roster, reports the existing
draft failure and consumes no IDs; Reset defaults can repair that draft.

State protocol becomes **v25** for the descriptor shape and allocator metadata;
browser/native/server/publisher identifiers agree. Release-control v1 remains the
opening compatibility boundary. The explicit persisted v24 decoder reads the
canonical old descriptor shape, derives the next ID from the entire old ledger,
retains live/draft records and drops unused state. Existing ID-derived property
keys are unchanged, and previous IDs are never reused. Signed app schemas and
active content values are preserved. Keep a matching verified v25 code recovery
kit; older codecs are not rollback targets after migration.

Focused acceptance covers repeated create/remove beyond the former lifetime
limit, bounded current checkpoint size, fresh properties on reused slots, delayed
operations and retries, pinned snapshots, full-ledger migration, restoration,
layout/gesture retention and authority process recovery before live deployment.
