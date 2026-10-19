# OTriOS architecture proposal

OTriOS uses Linux for hardware support, drivers, process isolation, and memory mapping. A userspace runtime adds immutable objects, capability-controlled interfaces, and reactive data flow. All mechanisms below are proposed unless the README identifies them as implemented.

## Object identities and snapshots

A logical ObjectId identifies an evolving entity. Each revision has a ContentId computed with SHA-256 over a versioned canonical envelope containing the schema identifier, schema version, and exact payload bytes. Specify byte order, alignment, field presence, and validation limits before implementing hashing. A revision hash proves byte identity, not authority or access rights.

An aggregate snapshot is an immutable manifest mapping ObjectIds to revision hashes, with one generation and one publication authority. Stage all referenced revisions, validate their existence and bytes, then publish the entire manifest atomically. A reader pins one manifest and sees its complete state until explicitly advancing. Old manifests remain valid while pinned.

For the PoC, one daemon serializes publish requests. Each request includes the expected generation; a stale generation fails without partial state. This demonstrates one aggregate's atomic publication, not arbitrary cross-aggregate transactions, distributed consensus, or crash durability.

## Shared immutable memory and lifetime

On Linux, the proposed data plane uses sealed memfd objects passed to clients over a Unix domain socket. Seal writes and resizing before sharing; map payloads read-only. Verify the digest and envelope before publishing a handle. Use offset and length fields rather than process-local pointers, with checked arithmetic and bounds validation.

The authority must hold committed objects until no pinned snapshot references them. The PoC may retain all objects until shutdown with an explicit byte limit; a rejected allocation must not alter the published generation. Kernel descriptor lifetime preserves a reader mapping if the daemon exits, but authority restart does not imply cursor continuity or recovered state. Garbage collection and durable restart need later protocols.

## Reactive views and cursors

The PoC view is a named aggregate's ordered manifest; filtering and derived queries come later. A cursor identifies one generation within an authority epoch. Subscription registration and initial-snapshot capture happen under the same publication lock so no update is lost between reading and subscribing.

Each subscriber has a bounded notification queue. Coalesce updates to a latest-generation hint where possible; if the client has missed generations, return ResyncRequired and require a full manifest refresh. The runtime promises coherent snapshots, not delivery of every intermediate revision or exactly-once notifications. Define queue size, resume semantics, and maximum pinned bytes in the IPC RFC.

## Unified interfaces and capabilities

The intended IDL describes data, interfaces, and capability requirements in one schema system, with FlatBuffers-like validated access to shared bytes. Existing Rust structs are a temporary source-level contract, not the wire encoding. Compare a FlatBuffers-based prototype with requirements in interfaces/README.md before adopting or building a compiler.

Capabilities are authority-issued grants for operations on named resources. Neither a ContentId nor a client-provided boolean is a capability. Authenticate local clients, validate bounds before deserialization, and check grants server-side for every operation. The first IPC demo is same-user research code; production sandboxing, revocation, delegation, and cross-user access remain separate milestones.

## Streamable executable modules

A later module manifest binds content hashes, interface versions, imported capabilities, target constraints, and signer identity. Verify trusted signatures, the full referenced content graph, and policy before execution. Streaming may fetch chunks lazily; it must never execute unverified bytes. Define trust roots, revocation, rollback policy, and resource limits in a security RFC.

Portable compilation is a later experiment comparing candidate portable IRs and native execution. Native cache keys must bind module hash, compiler version, target architecture/features, runtime ABI, and compilation options. Treat cached output as an untrusted optimization until validated. No portable IR, compiler, or signing scheme is selected in this scaffold.

## Transactions and CRDT acceleration

Aggregate publication is the first transaction primitive. Optional transactions across multiple aggregates need an explicit isolation level, validation strategy, conflict semantics, and crash model before implementation.

CRDT resolution will use a reference software implementation and a permissively specified accelerator interface. CPU, SIMD, GPU, firmware, and ASIC implementations must agree on canonical output and error behavior, with deterministic conformance vectors and fallback. Validate algebraic properties appropriate to each CRDT, including convergence, associativity, commutativity, and idempotence where required. Accelerator access must preserve capability, memory, and version checks. No claim of universal hardware speedup is made.

## Decisions still requiring experiments

Select a concrete IDL and schema evolution rules; measure serialization versus direct mapping; define cursor epochs and reclaiming pinned revisions; compare portable compilation targets; specify distribution and LGPL relinking arrangements; and select CRDT families before designing an accelerator ABI. Keep each decision in a small RFC with testable alternatives.

## Incremental CRDT hierarchy experiment

[RFC 0002](rfcs/0002-incremental-crdt-hierarchy.md) proposes immutable typed CRDT leaves and incremental ancestor hashing, with measured hash-path costs on the frame. The bounded [RFC 0005 library](rfcs/0005-merkle-subtree-prototype.md) now implements immutable incremental nodes, causal register joins and subtree transport; the dashboard continues to use Automerge. Hash equality includes merge metadata and never replaces conflict semantics or access policy.

[The app package prototype](../apps/README.md) implements the live `Canvas(uiRef, stateRef)` mount with independent app documents, binary UI/selectors and a bounded Wasm behavior interpreter. [RFC 0004](rfcs/0004-canvas-behavior-and-app-packages.md) records its execution and package boundaries; all seven Home/demo apps now use those mounts. General Merkle integration remains a separate cutover.

Follow the [modern app pattern and retirement map](MODERN_APP_PATTERN.md) for new
app work. It separates the production target from current prototype evidence and
lists which legacy paths should be deleted at each cutover.
