# Authority, working state and immutable caches

The dashboard authority owns durable Automerge history, validates public mutations, and flushes/replaces its state file before acknowledging accepted changes. Monitoring policy remains server-side and its authorized snapshot stream is separate from public demo sync.

Browser and native clients start with a fresh seed and subscribe. Application documents, local input, optimistic values, private view snapshots and pending edits live only in RAM. A temporary disconnection can recover while the process lives; closing or crashing loses unacknowledged edits. This deliberately provides no durable offline outbox. The server and its backups are the recovery source. On reconnect Automerge exchanges missing history; it does not require the client to have retained a checkpoint.

Automatic code updates wait for server-confirmed document heads and no active gesture; native pending intent must also be applied. Native stop makes a final sync attempt with a bounded three-second wait, with no disk fallback. A manual forced stop can still lose pending work. Navigation transfers in the URL or native RAM runtime notice; Input Lab text remains local and resets on reload.

## Immutable artifacts

The web service worker manages at most **512 MiB of payloads**, reduced to a quarter of reported origin quota and available headroom when smaller. It keeps LRU metadata only in a separate artifact database. It admits only same-origin immutable public release assets and content-addressed `/objects/SHA256` payloads; requests for APIs, state, auth, Monitoring, ephemeral ink, ranges and authenticated resources bypass it. Blob bytes must match their SHA-256. Each item is limited to 32 MiB. Quota failures fall back to network delivery. Browser HTTP/JIT caches and metadata are outside this application payload budget; browsers can evict best-effort storage or grant less space. No persistence permission is requested.

The frame supervisor manages a **2 GiB** cache, configurable by `--cache-bytes`. It counts all cache files and evicts least-recently-used immutable digest entries before admission. The active and rollback releases stay pinned while a candidate is verified. Oversized protected sets reject admission and retain the running app. Signed manifest and executable hashes are rechecked before execution. The optional `cached_blob` adapter verifies SHA-256 and size and shares the same budget. There is no general blob storage service yet; this defines cache behavior for immutable object consumers.

Ready, stop, code-notice, route and diagnostic capture files live under `/run/otrios-direct-console/otrios` (tmpfs) on the frame. The persistent directory contains verified code, optional blob cache, and last-good artifact metadata. Legacy client document/Monitoring/route files are retired only after their last state is merged into the authority during migration. They are never a recovery source afterward. Server state and signing keys are not cache entries.

## High-frequency input

Local preview follows presentation timing. Replaceable control values publish at most every 50 ms plus exact endpoints; native pending absolute values supersede older unsent values independently per control, with local sync generation batched at 100 ms and immediate protocol responses for received updates. Ordered discrete actions retain order. Once committed, Automerge changes remain history; transport batching is not compaction.

Touch Pad motion instead uses the bounded ephemeral `/demo/ink` channel. Peer/stroke IDs preserve separate drawings, late joins preserve age, and marks fade five seconds after the stroke’s latest point. Only the final pad position is committed to durable app state. Trails are never stored in either server history or client caches. See [the ink protocol](TOUCH_PAD_INK.md).

## Validation

The native application worker is the sole mutable CRDT owner; its network thread moves ordered binary frames through bounded channels, never complete document replicas. Connection epochs reset sync state and discard stale transport frames after reconnect. Ready diagnostics report receive-to-present time for actual remote CRDT changes.

Core acknowledgement tests cover new edits, fresh sessions and fresh subscribers. Real server-process tests confirm an acknowledged edit is on disk before acknowledgement and survives killing/restarting the authority, then appears in a new RAM-only subscriber. Native worker tests prove a legacy state file is ignored and unchanged, even when the application worker is blocked during a drag. Cache tests cover exact byte accounting, LRU/pinning, quota failures, corrupt blobs, symlinks and bounded streaming. Ink integration tests use public and native WebSockets, late joining, expiration and unchanged durable history/private view.

The proposed [incremental CRDT hierarchy](rfcs/0002-incremental-crdt-hierarchy.md) defines the next runtime experiment; it is not implemented by the current Automerge host changes.
