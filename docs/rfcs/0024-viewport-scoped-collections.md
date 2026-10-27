<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0024: Viewport-scoped collection synchronization

Status: implementation in progress. Delivery 1 is qualified with a large synthetic
source over the shared session on macOS/Linux, with bounded process/scaling
evidence. The live chat/block cutover remains incomplete. See the
[implementation ledger](../experiments/collection-windows-2026-10-28.md).
Clarifies the user's pervasive-sync objective; not implemented by the chat UI
fixes. Extends [RFC 0003](0003-module-roots-and-state-selectors.md),
the existing partial Merkle replica, and [RFC 0023](0023-retained-presentation-foundation.md).

## The missing boundary

A long chat thread should be a first-class collection in OTriOS's shared data
model. A client synchronizes an authorized, coherent view of that collection,
including scrollback, rather than downloading the full thread or implementing a
separate application-specific history protocol. Server authority still controls
admission, ordering and moderation. CRDT replication does not grant permission.

Today Canvas uses covered Merkle synchronization for bounded typed fields. Chat
instead uses a separate ordered journal and history commands over channel 3.
That is a bounded prototype shortcut, not the intended reusable data mechanism.
Keep its durability and retry guarantees during the cutover, then retire its
history-specific replication path. Presence, expiring drafts and particle events
remain ephemeral; they do not become durable log or per-keystroke CRDT writes.

## Logical collection, selected view, local presentation

```text
Thread root: ordered membership index + message records by stable identity
       ↓ authorized typed collection selector and bounded lease
Coherent selected membership + required records + causal merge metadata
       ↓ partial RAM replica and immutable local message blocks
Reflowed visible blocks + overscan → placements/clips → retained GPU resources
```

The data lease uses record keys/cursors. Pixel positions belong to local layout:
different clients may have different widths, fonts, keyboards and scroll offsets.
The viewport maps visible blocks and overscan to a bounded key window. A resize
can reflow the same selected records without resubscribing or fetching the log.

Each message has a stable ID independent of its position, sender nickname and
rendered height. Keep immutable sent-content/profile snapshots as now; later
edits/deletions require explicit versioned record and tombstone semantics. A
server commit sequence is a useful order key under the current single authority.
Concurrent insertion or editing from independent authorities requires a defined
ordered-collection CRDT, not wall-clock sorting or hash order. This proposal does
not quietly add that distributed write model.

An incremental ordered membership index supports seeking around an ID, before a
cursor, or at the tail without scanning all messages or sending every key. Its
internal nodes and records must fit independently bounded canonical Merkle
objects. A giant register containing all messages or all message IDs defeats
partial synchronization. An index page is a storage unit, not a UI identity.

## Lease and coherence contract

A proposed collection selection is conceptually:

```text
thread.messages.window(anchor = Tail | Before(cursor) | Around(messageId),
                       recordLimit, byteLimit, requiredFields)
```

Readable notation is not a new text query parser. Compile a schema-checked plan
with explicit resource scope, authorized fields and strict limits. Reuse the
existing coverage/partial-join machinery where its invariants apply.

Each accepted view carries an authority epoch, lease/request generation, pinned
collection revision, selected ordered membership, and coverage needed to merge
its records coherently. A stale response cannot replace a newer requested view.
Membership and referenced record versions must qualify before publishing the
new view; never mix old ordering with independently advanced row contents.
Canonical hashes establish byte identity and synchronization equality, including
causal metadata. They are not signatures, capabilities or proof of admission.

Changing a window establishes and qualifies bounded new coverage before releasing
old coverage. Bound that overlap, coalesce superseded window requests, cancel
obsolete transfers, and report backpressure/resync explicitly. A client need not
receive every intermediate update. Reconnect includes current release discovery
independently of recovering its selected data view. Resume cursors are scoped to
the authority epoch and retention policy, with an explicit anchor-expired result.

When reading the tail, subscribe to new tail membership. When reading older
history, retain its anchored window and optionally subscribe to small head/tail
metadata for an unread badge. An unrelated message outside the selection should
not transfer its body or invalidate row layout. Permission changes revoke or
replace coverage explicitly, including immediate clearing of private data.

## Scroll position, caches and row integrity

Anchor scrolling by `(messageId, content revision/source position, viewportY)`.
Prepending a history page or discovering wrapped heights preserves that anchor;
only explicit user intent or tail-follow changes it. Tombstones or expired
anchors choose a documented neighboring key and report that adjustment.

Text, sender label, avatar and inline content are one immutable message block.
They share the block's placement, clipping and source anchor. A separate cached
avatar overlay driven by another invalidation clock is not sufficient. Until
block resources replace it, every pointer-induced viewport movement must refresh
the overlay in the same presentation turn as the text.

Keep selected records, causal merge workspace, layout/chunk caches, transferable
snapshots and GPU residency under separate explicit budgets. Retain a bounded
overscan/reverse-scroll reserve; evict unselected records and disposable layout
resources without pretending that unselected data is deleted from the collection.
Scrolling back farther reacquires coverage by stable cursor. No durable browser
copy of the whole thread is required. Pending local sends remain a separate
bounded intent queue until the authority durably acknowledges them.

## Delivery and acceptance

1. **Reusable collection views:** a paged ordered index, schema-checked bounded
   window selections, coherent partial replicas and lease replacement on the
   existing shared synchronization transport. Qualify one large synthetic thread
   before changing the live chat store. Test separate processes, sparse windows,
   concurrent publication, slow readers, superseded requests, cursor expiry,
   reconnect, permission revocation and canonical-byte convergence. Measure
   transferred records/bytes and memory as total history grows while window size
   stays fixed; no full-log key scan or transfer may hide in initial selection.
2. **Chat and structured blocks:** move chat history through those collection
   views and render stable message blocks, retaining existing durable receipts,
   guest identity and moderation. Preserve anchors across prepend/append/reflow,
   compare text/avatar clipping and pixels, and measure actual native page flips.
   Recover the existing journal into the new authoritative representation with
   one publication owner, a rollback point and restart tests. Remove the old
   history commands/client paging/store implementation only after the cutover;
   do not maintain two writable log owners.

Any changed state wire contract must bump the declared protocol on browser,
native and authority together. The stable v1 release-control hello remains
unchanged. Merely using the same WebSocket does not complete this RFC.
