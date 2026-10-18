# OTriOS proof of concept plan

Validate whether independent Linux processes can share immutable object revisions, publish a coherent aggregate snapshot, and react to updates without observing torn state. Use a userspace daemon and two clients; retain Linux's existing driver and scheduling model.

## Active dashboard experiment

The user requested a shared touch dashboard after this initial research plan. [DASHBOARD.md](DASHBOARD.md) defines that bounded product experiment: native GLES2 on the Rockchip frame, WebGL in a browser, a shared Rust compositor, CRDT edits and complete immutable view snapshots. It does not satisfy P1–P5's Linux IPC/shared-memory acceptance gates. The milestones below remain the runtime research backlog.

## Scope

Rust 1.90.0, Linux x86_64 first, aarch64 as a follow-up; one local authority, one named aggregate, one writer, and two readers. Each update replaces two related counter objects in one manifest. Readers must always observe equal counters within a pinned snapshot, even during rapid publication. The same mechanism supports more objects, but this demo is deliberately small.

The scaffold's existing unit tests validate manifest behavior only. They do not demonstrate IPC, atomic concurrent publication, content validation, or shared memory.

## Milestones and acceptance

| Milestone | Deliverable | Required evidence |
| --- | --- | --- |
| P1 Content store | Canonical envelope, SHA-256, deduplication, size limits | Known digest vector; equal bytes deduplicate; distinct schema/version changes digest; corrupt and missing content rejected; old bytes preserved |
| P2 Snapshot authority | Staging, pinned manifests, expected-generation publication | Two publishers racing the same generation produce exactly one success; stale requests and missing revisions leave state unchanged; old readers retain old values |
| P3 Linux IPC | Daemon, SDK client, sealed memfd data plane, validated socket control messages | Writer and two separately spawned reader processes; read-only mappings; write/resize seals; malformed offsets/lengths rejected; daemon shutdown behavior documented |
| P4 Reactive views | Atomic subscribe/read registration, bounded queue, cursor advancement | No registration gap under a stress schedule; monotonic observed generations; paused reader gets explicit resync; pinned old snapshot unchanged |
| P5 Demonstration | Reproducible demo and benchmark report | Linux integration and CI pass; demo checks 10,000 two-object publications with two readers and zero torn snapshots; memory growth and cleanup behavior measured |

Implement and review P1 before building P2, then P3 and P4. P5 is the completion gate for this PoC, not a requirement for the initial scaffold PR. Keep follow-up changes small enough to review independently.

## Failure and resource model

Every publish checks its expected generation and validates all referenced revisions before changing the current manifest. Failed publication exposes nothing. Reject oversized envelopes and manifest counts before allocation. Use checked integer arithmetic throughout message parsing.

The first daemon is memory-resident and may retain revisions until exit, with a configured total byte cap. Exhaustion rejects writes explicitly. Slow readers may skip intermediate generations only with a resync signal. No durability or restart continuation is promised. Specify whether a pending request failed or has an uncertain outcome if the connection closes; retries must reread current generation.

## Tests and benchmarks

Add Linux integration tests that spawn real child processes and use temporary sockets. Use deterministic barriers for race tests, timeouts to prevent hangs, and seeded stress runs with the seed printed on failure. Test both sides of size limits, invalid handles, denied operations, disconnects, and queue overflow.

Measure publish throughput, p50/p95/p99 publish-to-observe latency, resident memory, mapped bytes, and bytes copied per update. Vary 1 KiB and 64 KiB payloads, 1 and 16 subscribers, and an intentionally paused subscriber. Record kernel, CPU, build profile, seed, warm-up, repetitions, and revision. Compare a socket-copy baseline where practical. Report measurements rather than inventing a speed target.

## Deferred work

Executable streaming and signing; native code caches and portable compilation; custom kernels or drivers; persistent storage and recovery; distributed operation; arbitrary queries; general multi-object transactions across aggregates; CRDT merge algorithms; GPU/ASIC acceleration; production capability security; stable ABI and SDK release.
