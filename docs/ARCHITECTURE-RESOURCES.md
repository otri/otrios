<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas resource ownership and bounds

The app owner is the only mutable replica/intent owner. Renderer/presentation
threads consume immutable views and own their clocks, focus/caret, touch previews,
ink, font textures and GPU buffers. The authority owns durable documents, policy
and atomic receipt publication. Clients never checkpoint documents to disk.

`Client::resource_usage` is sampled on the app-owner thread. Browser workers send
it as `frame.resources`; the DOM stores `data-runtime-resources` on the revision
indicator for inspection. Native readiness reports expose `runtime_resources`.
It includes pending/issued field payloads and ages, dispatch deadline, selected
canonical bytes/nodes, actual replica/document root sharing, eight-entry behavior
cache bytes, and current-thread codec scratch capacities/growth. The sample has a
monotonic timestamp. It updates with published app views, not a separate poller.
No private values or credentials appear in these diagnostics.

These counters are **not RSS, heap allocation counts or exact unique live bytes**.
Canonical tree counts include encoded metadata; Vec capacity includes slack;
shared roots must not be summed as independent allocations. GPU/guest/container
and allocator overhead are separate. Compression scratch stays zero on the normal
raw replication path unless an explicit compressed packet is processed.

| Owner | Retained work / bound | Overload behavior |
| --- | --- | --- |
| App intents | One pending map + one immutable issued write, combined 256 KiB typed payload, at most 64 fields per write | Reject whole assignment before mutating intents; final gesture flushes the last value; ACK bypasses throttle |
| Canvas document | Canonical bytes capped below 512 KiB (8 KiB packet headroom), receipts at most 1024 writers | Reject atomic candidate; read-only leases consume no receipt slots; no silent receipt eviction |
| Generic Merkle tree | 16 MiB canonical, 16,384 nodes, depth 16, 32 children/actors, 256 edits | Validate before publication; generic limits are ceilings, Canvas has the tighter document limit |
| One subscription | One projected view, one acknowledged root, one immutable flight/packet; no revision queue | Retransmit same flight, then coalesce to latest after ACK; explicit resync/revocation |
| One server peer | Active + staged lease; eight input packets, four output packets; each ≤512 KiB | Queue overflow closes for reconnect; output send has a deadline |
| Decoding | ≤512 KiB expanded packet, 8 MiB charged per monotonic one-second fixed window per session | Charge before expansion, close/reconnect on excess; backwards time cannot refill budget |
| Optional LZ4 | One per-thread decode buffer ≤512 KiB, one bounded maximum-output buffer, one boxed 16 KiB table | Invalid declared length rejected before growth; exact block length/digest checked; no nested expansion |
| Native host | Eight network replies, one send slot plus one pending send, one replaceable presentation slot; independent bounded input/ink queues | Wire order retained; presentation views coalesce; transport backpressure remains bounded |
| Browser host | Four send credits, bounded receive bridge and one in-flight + one latest presentation view | Transfer buffers on ownership handoff; reconnect on overflow; renderer clock is independent |
| Behavior | Eight cached binaries, each ≤64 KiB; one admitted active instance, one memory ≤64 KiB, no tables, dispatch fuel 10,000 | Validate hash/imports/ABI and fuel before accepting commands |
| Authority host | 64 total session permits, 32 public permits; eight catalogs, 128 app roots | Reject admission/refresh beyond budget; no client-created arbitrary room names |
| Journal per root | Rotate checkpoint at 4 MiB, maximum journal 16 MiB, record ≤512 KiB | Durable failure leaves prior publication authoritative; corrupt complete records fail closed |
| Artifact caches | Browser target 512 MiB, native target 2 GiB; disposable immutable verified code/font/blob resources | Evict least recently used resources; storage unavailable never makes document state durable on client |

A session's addressing/projection roots share immutable nodes. While a flight is
outstanding it does not compute/queue a new projection. A lease may pin an older
full authority root until its next offer; the live room root may have advanced.
ACK, flight and projected snapshots can also share roots. Two staged/active
leases deliberately retain more than a single selected tree during qualification;
rejection/replacement drops the old activation. The table is an ownership model,
not a promise that adding every ceiling gives an exact process bound. Per-peer
packet queues alone may retain several MiB; the peer semaphore is also essential.

Meaningful overload coverage lives in core dispatch/transport/Merkle tests and
server process tests: immutable-flight charging, atomic payload rejection,
malformed expanded packets, fixed-window exhaustion/backwards clocks, slow-reader
coalescing, partial/corrupt publication, eight-queue credited bursts, auth expiry,
policy denial and journal crash recovery. Host worker tests hold state processing
while presentation/input feedback continues. Physical touch acceptance remains a
separate fact from these automated checks.

The JSON serializer added to the WASM host is the same pinned `serde_json` already
in the workspace lock; it serializes the shared diagnostics type rather than
maintaining separate numeric field layouts in JavaScript and native code.
