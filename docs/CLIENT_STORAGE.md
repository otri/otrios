<!-- SPDX-License-Identifier: Apache-2.0 -->
# Authority, RAM working state and immutable caches

The authority owns complete durable Merkle documents, validates assignments and
persists changed nodes plus writer receipts before confirmation. Schema, field
write policy and Monitoring access are enforced server-side. Read-only leases
need no durable writer receipt. Canonical state stays on the server; clients
materialize only their admitted coverage.

Client replicas, the pending map, immutable issued write, optimistic values and
private views live in RAM. Reconnect creates a lease and resynchronizes against
the authority while the process retains pending edits. Closing or crashing may
lose unconfirmed edits. There is no durable offline outbox or document cache.
Confirmed Input Lab text survives through server state; caret/focus/repeat and
animation phase are local and restart on reload.

Connected code updates wait for confirmed intents and finished gestures. A
currently disconnected client may discard unconfirmed RAM edits when upgrading,
as explicitly allowed by the PoC policy. Native stop makes a bounded final sync
attempt and never saves a client checkpoint. Preservation of offline edits is a
future requirement, not an implied feature.

The browser artifact cache targets 512 MiB of payloads, reduced by available
origin quota/headroom. The native agent defaults to 2 GiB for disposable
code/font/blob caches; the checked-in frame service overrides it to 128 MiB.
Immutable verified resources are evicted by recency; active and last-good native
releases are pinned. Cache failure falls back to resource delivery rather than
making app state durable locally. Browser HTTP/JIT caches and metadata, GPU glyph
textures and process heap are separate from these application payload budgets.

Authenticated/private/API/state/ink responses bypass the public browser artifact
cache. Public release assets bind their size and digest in the release manifest;
content-addressed blobs verify SHA-256. Native execution also verifies the pinned
publisher signature. Neither a hash nor cache possession is an access grant.

See [resource ownership and limits](ARCHITECTURE-RESOURCES.md),
[packed transport](rfcs/0011-packed-canvas-transport.md), and
[session/upgrade ownership](DASHBOARD_LIVE.md). The earlier Automerge checkpoint,
IndexedDB document cache and migration fallback are removed from the app runtime.
