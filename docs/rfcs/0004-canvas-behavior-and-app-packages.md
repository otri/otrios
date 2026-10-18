<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0004: Canvas mounts, executable behavior and app packages

Status: scoped prototype authorized by the user. Builds on RFCs 0002/0003.

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

The first independently mounted apps are Home, Component Playground and Layout
Lab. The other existing demos get package manifests recording their legacy
adapter status and current dependency locations. They are not represented as
migrated independent replicas until that migration is implemented and tested.

## Prototype boundary

An opt-in Canvas server/viewer uses separate state documents and UI packages for
each app; it does not replace the working dashboard or migrate its live history.
The server binds loopback by default and accepts its exact browser Origin or a
native client with no Origin. Its local package registry is trusted prototype
configuration, not production publisher authentication. UI/behavior hashes are
verified before activation. Production delivery must bind the graph to the
existing pinned publisher/signature policy before exposing editable imports.

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
Candidate UI/code is qualified before replacing the active instance. If validation
fails, the prior view/document remain retained but input is suspended until
reconnect; transactional remote-mount rollback remains follow-up work. Immutable
code bytes can be reused; mutable instance memory never leaks across State
inputs. Reopening an app resubscribes from the authority rather than a disk
checkpoint. Durable state stays behind the stable State reference.

UI/code updates require a compatible state schema and no active input/pending
unacknowledged edits before activation. A rejected candidate leaves the old view
retained read-only until reconnect. Navigation disposes the old subscription; independent apps do not
receive each other's payloads. UI/code are hashed as canonical artifact bytes. State uses Automerge history
and causal sync; the prototype does not implement RFC 0002 incremental Merkle
state hashing or per-selector subtree subscriptions. It scopes subscriptions at
app-document roots. Flat stable field-ID selectors are implemented; collection
paths, item identities and cursor queries remain RFC 0003 design work.

The package and migration guide is [apps/README.md](../../apps/README.md). The
first implementation uses a build-time registry and explicit package reload on
mount. Adding external packages, automatic update notifications and production
signed app admission remain separate milestones.

## Acceptance

Run a separate browser viewer and native headless process against a real server.
Prove slider/percentage share one field, behavior bytes are fetched/verified and
executed, navigation changes UI/State roots, two State inputs sharing UI stay
independent, inactive-app updates are absent, reconnect recovers acknowledged
state, and stale events/wrong hashes/ABIs/imports/fuel exhaustion/bad commands
are rejected without changing state. Render through the existing primitive
compositor. Report headless native evidence separately from physical display
acceptance. Keep the existing live dashboard intact during the opt-in prototype.
