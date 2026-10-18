<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0004: Canvas mounts, executable behavior and app packages

Status: app-scoped v2 cutover implemented; deployment/physical acceptance recorded separately. Builds on RFCs 0002/0003.

`Canvas(uiRef, stateRef)` is a mounted UI description and its explicit state
scope. A UI description includes primitive layout data, typed selectors,
responders, navigation references and optional content-addressed behavior imports.
Nested layout canvases inherit scope rather than creating network subscriptions.

Apps live under `apps/<app-id>/`: `app.rs` is typed build-time manifest authoring,
`ui.rs` authors binary UI data, `state/schema.rs` owns the app's schema/defaults,
`behavior/` contains executable sources and `package/` contains generated binary
artifacts. Optional assets/tests stay with their app. Shared interpreter,
compositor, selector and CRDT implementations remain in LGPL `otrios-core`;
permissive public contracts remain in `otrios-interfaces`. Apps declare local
build/runtime dependencies explicitly. A launcher has opaque app/UI/state
references, never an implicit import of every app's implementation or history.

Home, Component Playground, Layout Lab, Input Lab, Animation Lab, 3D Lab and
Monitoring now have independent packages and State references. The production
build uses the same Canvas runtime on web and direct GLES. The former global
room and fixed app Shell are retired from that path; this cutover starts fresh
app documents rather than running a legacy mirrored authority.

## Admission and transport

A public signed `apps/catalog.flex` binds app identities, UI/State references,
privacy requirements and UI hashes. Each admitted UI binds behavior imports.
Signed UI/code payloads stay in the release's `authority/apps/` tree; the public
module replica never receives those files. `/apps/stream` checks the admitted
pair and server-side permission before sending Mounted UI/code/state. Browser
Monitoring sessions are revocable; the trusted native listener retains its
private transport policy. App-provided requirements are not grants.

The authority persists accepted state before acknowledgement. Clients hold
replicas and pending edits in RAM, preserving them through transport reconnect.
Only immutable public code/blob resources enter bounded persistent caches.
The browser verifies catalog bytes against its immutable release manifest;
existing module-replica/native-supervisor code verifies publisher signatures.
A loopback package loader remains useful for tests, not a separate production
auth or storage service.

UI format `otrios-canvas-v2` supports fixed typed fields, including bounded text,
booleans, colors and series. Input Lab text uses whole-register conflict semantics,
not per-character collaborative editing. Keyboard focus, selection and repeat are
local. Monitoring's separate, read-only binary Layout aggregate is replaceable
and never appended to CRDT history. Animation/3D sync intent while presentation
phase, clock, GPU resources and the isolated RAM CRDT stress workload stay local.

The selected interpreter is Wasmi 1.1.0 (Rust 1.86 minimum, compatible with pinned
Rust 1.90), with binary parsing only. It runs the same behavior bytes in the
browser's outer Rust/Wasm runtime and native ARM host. A pinned WAT compiler is
build/test tooling only. No JIT, WASI, filesystem/network imports or new unsafe
boundary is added. The initial guest is authored as WAT, avoiding unsafe Rust
export attributes and keeping the ABI visible. This is an execution experiment,
not portable-IR selection or a general sandbox guarantee.

A behavior import declares its SHA-256, ABI version and permitted state fields.
The host enforces byte, memory, table, stack and fuel limits. A guest returns
bounded typed commands, never mutates host state directly. The authority checks
every new CRDT operation against its own schema/policy and persists before sync
acknowledgement. Client-provided permissions are not grants.

Event/state reducers, local presentation/animation and external effects are
separate roles. Only pure, synchronous event reducers are admitted initially;
there are no effect imports. Declarative bindings and standard field setters
work without custom code. Derived formatting remains read-only.

## ABI v1

`abi_version() -> i32` must return 1. `dispatch(event:i32, a:i32, b:i32,
current:i32) -> i64` returns a command mask and up to five normalized register
values, each in 0..1000. Five ten-bit lanes occupy bits 0..49; a five-bit write
mask occupies bits 50..54. Other bits, unknown events and unauthorized fields are
rejected. A zero mask is a no-op. This tiny fixed ABI avoids guest-memory pointers;
a general typed command-buffer ABI needs a later version.

Each Canvas activation owns its guest instance, sync session, input capture and
scope generation. Late commands/messages from a replaced or exited scope are
ignored. Trap/fuel exhaustion leaves the document intact and fails that call.
Candidate UI/code includes authoritative state and is qualified before replacing
the active instance. Rejected updates retain the previous mount. Revocation drops
a private mount immediately, while keeping only the references needed to request
access again. Immutable code bytes can be reused; mutable instance memory never leaks across State
inputs. Reopening an app resubscribes from the authority rather than a disk
checkpoint. Durable state stays behind the stable State reference.

UI/code updates require a compatible state schema and no active input/pending
unacknowledged edits before activation. Navigation disposes the old subscription;
independent apps do not receive each other's payloads. UI/code are hashed as
canonical artifact bytes. State uses Automerge history and causal sync; the app
runtime does not implement RFC 0002 incremental Merkle
state hashing or per-selector subtree subscriptions. It scopes subscriptions at
app-document roots. Flat stable field-ID selectors are implemented; collection
paths, item identities and cursor queries remain RFC 0003 design work.

The package guide is [apps/README.md](../../apps/README.md). The build registry
and signed release catalog are explicit. Code release notifications wait for
confirmed state and idle input before replacement. General external package
discovery and arbitrary capability imports are not implemented. Current bounds
include 64 fields, 4 KiB text, 2,016 series points and 512 KiB messages/artifacts;
history has a smaller byte ceiling for transport headroom. Exhaustion fails
explicitly; general history compaction is not implemented.

## Acceptance

Run a separate browser viewer and native headless process against a real server.
Prove slider/percentage share one field, behavior bytes are fetched/verified and
executed, navigation changes UI/State roots, two State inputs sharing UI stay
independent, inactive-app updates are absent, reconnect recovers acknowledged
state, and stale events/wrong hashes/ABIs/imports/fuel exhaustion/bad commands
are rejected without changing state. Render through the existing primitive
compositor. Report headless native evidence separately from physical display
acceptance. Report a staged build separately from a deployed cutover; old clients
require a new entry visit when the protocol changes. Test private package denial and
revocation, read-only aggregate replacement without CRDT growth, and pending
RAM edits across reconnect as part of the app cutover.

## Declarative source authoring

App sources compose `canvas`, `text`, `rounded_rect`, `circle` and `button` values
with modifiers. A button carries its destination; an event responder carries its
field focus/guest event; a visual property carries its typed field projection.
The packer generates node IDs, local selector slots, navigation indices and event
control indices together with their tables. These wire details are not authored
as separately paired numbers. Identical field/projection plans are interned.

Durable schema field IDs and guest ABI event IDs remain explicit, named and
versioned. UI node IDs are deterministic structural indices within a compiled UI
revision/activation, not durable component keys. Reordering can change them;
stale input generations cannot target a replacement UI. Stable logical keys for
collaborative UI-graph editing and a general stack/constraint layout language are
not implemented by this source builder. Binary UI transport/runtime interpretation
remain unchanged. See the app guide for concrete source examples.

[RFC 0005](0005-pins-and-struts-layout.md) adds parent-local pins, centers and
spring/strut resizing to the same binary primitive descriptions, with reusable
component design sizes and layout-v2 admission.
