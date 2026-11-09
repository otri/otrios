<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0036: Independently upgradable OIB component libraries

Date: 2026-11-09. Status: **L1 implementation in progress; live qualification pending**.

The user selected independently upgradable OIB libraries on 2026-11-09. This RFC
defines that boundary and a bounded first loader milestone. The
[L1 work record](../OIB_LIBRARY_L1.md) tracks implementation and qualification.
D2 itself did not import libraries or execute library behavior.
[D2 completion](../OIB_D2_COMPLETION.md) is delivered; L1 is the next loader milestone.
The practical boundary is summarized in [OIB library boundary](../OIB_LIBRARY_BOUNDARY.md).

## Goal and current execution

Routine component appearance, composition and eventually interaction updates
must be publishable without rebuilding the Frame executable, browser host module
or authority executable. The runtime supplies generic scene, layout, input,
state, publication and sandbox mechanisms. Libraries supply reusable definitions,
styles, exposed properties, slots and, in a later stage, portable behavior.

Today the Frame runs a native ARM host. Optional app `Code/behavior.wasm` runs in
Wasmi with the bounded app behavior ABI 1. The browser executes the outer OTriOS
host WebAssembly with its browser engine; that host also uses Wasmi for nested
app behavior. Portable WebAssembly is the common executable format; a browser's
own WebAssembly engine and our embedded guest interpreter are separate layers.
Neither path dynamically loads component `.so` plugins.

Current OIB classes and input actions are bounded host contracts. D2's canonical
graph is a useful source/compiled-output boundary, but its closed `ClassKind`
and `Action { kind, Set/Toggle, source }` are not a general widget behavior ABI.
The existing app ABI's five command lanes are not that ABI either.

## Selected stages

| Stage | Scope | Host changes |
| --- | --- | --- |
| D2 | Canonical authored graph, semantic property identities, bounded structural edits | Deployed; no library imports |
| L1 | Signed data-only library imports, exact pins, compatible baseline adoption | Implement loader/adoption once; later compatible library bodies reuse it |
| L2 | Portable component behavior with a generic typed event/command ABI | Implement and qualify the ABI once; widget behavior then ships in libraries |
| Later XR | Spatial scene/input/backend support and XR libraries | Explicit runtime/backend milestone before XR library iteration |

L1 reuses admitted 2D shapes, expressions, layout, classes and actions. It can
change graphs, styles and parameterized composition within those contracts. It
cannot add a rendering primitive, font/image subsystem, new gesture semantics or
an executable widget implementation. L2 is a separate bounded OIB extension,
not activation of general executable modules, portable AOT or transactions.

## Package and dependency contract

A library has a stable publisher-scoped `LibraryId`, stable numeric `ExportId`s,
a human-readable version, and an immutable `LibraryRevision` content hash. The
version is descriptive; the exact SHA-256 revision selects executable/data bytes.
Names and content hashes never become object, definition, node or property IDs.

Use the signed immutable package discipline from
[RFC 0025](0025-standalone-app-bundles.md), with a distinct library package kind:

```text
Info.flex                  kind, LibraryId, version, format/host requirements
OIB/library.flex            canonical definitions, exports and exposed contracts
Code/component.wasm        absent in L1; optional L2 guest
Integrity/files.flex       exact admitted files and SHA-256 digests
Notices/                   applicable notices
```

Bind kind, format, publisher identity, requirements and all admitted files into
the signed package root using the existing package canonicalization/signature
rules. Do not invent a second signature system. A content hash is neither a
signature nor an execution grant. Verification uses the existing authorized
publisher/trust workflow; this RFC adds no trust roots or identities.

The library format must have its own fixed kind/major prefix and digest/signature
domain, rejecting an app package in a library slot and a library in an app slot.
Reuse the locked canonical integrity encoder, Ed25519 verification and external
publisher key policy; specify library header/domain bytes and conformance vectors
in L1 before generating packages. Never interpret existing `OTAPP` app metadata
as a library merely because it contains graph bytes.

L1 admits no native executable, compiled cache, filesystem path reference,
mutable app state, credentials or remote URL in a library. Resource files and
transitive library dependencies remain later extensions. Styles initially use
existing graph properties and primitives; do not introduce a parallel CSS engine.

An app's signed manifest carries an exact lock of `(LibraryId, revision)` and
the expected export contracts. Import bindings persist the document-local import
identity and `(LibraryId, ExportId) -> DefinitionId` mapping. Reusing a binding
with another revision is legal only after compatibility validation. Rebinding it
to another export is an explicit replacement with new semantic identities.

L1 supports at most four direct libraries and no library-to-library imports.
Reject duplicate logical libraries at different revisions, missing dependencies,
unknown required host/format versions and untrusted packages. Do not resolve
`latest`, floating version ranges or network dependencies during rendering.
The publisher stages a complete signed app/library closure and updates the app's
lock. This may republish app metadata but must not rebuild a host executable.
Automatic catalog-wide adoption and a general dependency solver are not L1.

## L1 codec selected for implementation

The public contracts are [library.rs](../../crates/otrios-interfaces/src/oib/library.rs)
and [graph.rs](../../crates/otrios-interfaces/src/oib/graph.rs). The package header is
`OTLIB\0\x01\0`, distinct from `OTAPP`. The package SHA-256 input is
`otrios-oib-library-v1\0` followed by the existing canonical integrity manifest.
Ed25519 signs `otrios-oib-library-signature-v1\0`, the raw trusted public key
and the binary package digest. The detached receipt format omits the trailing
NUL. [Package vectors](../../crates/otrios-core/src/library/tests.rs) lock these
bytes independently of the app package vectors.

`LibraryId` is `<publisher-public-key-sha256>:<slug>`; it is separate from the
32-byte package revision. Exports carry stable numeric IDs and derived semantic
contract digests. `otrios-oib-imports-v1` records exact locks and persistent,
complete source-definition to document-definition maps. The app manifest repeats
the same lock. Signed packages are nested under
`Dependencies/<revision>.otlib/`; the enclosing app's integrity closure covers
all files, and admission separately verifies each inner publisher signature.
No transitive library package is admitted.

Canonical app graph definitions stay local. Separately transported
`Seed.resolved_libraries` contains the exact verified immutable bodies;
resolution must agree with the locks, exports, class requirements and namespace
maps before lowering. Compiled renderer output never becomes this source.
The new recoverable binding is `otrios-oib-binding-v2`, with a fixed allocator
floor and a bounded last-adoption witness. State protocol advances to v30;
the opening release-control envelope remains v1. The initial format/import
cutover uses fresh disposable OIB collections. Compatible library updates after
that cutover retain the collection and semantic epoch.

## Authored source, resolved graph and identity

Keep three explicit layers:

1. Immutable signed library definitions/defaults, addressed by their package pin.
2. App-authored local definitions, import bindings, instances and connections.
3. A transient resolved graph compiled into layout, scene, selectors, semantics,
   responders and renderer handles.

Do not flatten imports into editable local copies or serialize renderer output.
Imported structure is read-only in an app document. Edit the library authoring
workspace to publish a revision, or create an explicit local component with new
identities. Local wrappers may compose imported exports and expose slots.

Existing `DefinitionId`, `NodeId`, `PropertyId` and full occurrence ancestry
remain semantic identity after resolution. The stable import binding supplies
the document-local namespace. Package revision, display name, renderer index and
compiler cache key are excluded from those addresses. Persist the binding map
before publishing occurrences; never reconstruct it by current export order.
Allocate imported definition bindings from the same monotonic document identity
allocator as local definitions; preserve each definition's stable node/property
IDs. L1 library exports reference the app's admitted class/property contracts by
ID and required semantic signature. They cannot overwrite those contracts or
introduce a conflicting class ID during resolution. Keep library-specific style
defaults on definitions/nodes rather than mutating a global shared class table.

A property with no causal assignment means **inherit the current default**.
Initialization must not eagerly write old authored/library defaults as override
registers. The effective precedence is an explicit occurrence override, preset
assignment/forwarding, app-authored shared override, then the pinned definition's
default. Existing validated app bindings still read app-owned state. A baseline
upgrade changes defaults beneath overrides; it does not emit synthetic user edits,
clear registers or rewrite causal heads. Reset removes the observed assignment
and reveals the newly pinned default according to the existing causal contract.

## Compatible baseline adoption

Current D2 `configure_oib` admits an identical signed graph or requires a fresh
collection. Keep that rule until the dedicated adoption operation exists. A
library loader must not obtain upgrades by weakening equality checks or resetting
every collection whenever a layout changes.

Separate the stable semantic admission contract from the resolved body/pin:

- **Semantic admission:** collection incarnation, app state schema, admitted
  field/action grants and stable property/slot/outlet identities and types.
- **Resolved baseline:** exact app source and verified library lock/body digests.
- **Active closure:** the resolved baseline plus the admitted authored/causal
  projection and compiled scene identity.

Compatible library bodies will advance the baseline and structural generation
while retaining the semantic epoch, app data, identities, causal contexts and
receipts. The D2 boundary cleanup separates a semantic-admission digest for state
schema, class/property signatures and signed action grants from graph body bytes.
The selected epoch binds document incarnation, scope, UI reference and logical
graph identity with that digest. Full source equality still gates configuration.
L1 must version its separate baseline lock/journal and wire representation; it
must not reinterpret an old epoch or substitute an epoch on a queued intent.
Incompatible schema/grant/semantic changes still require explicit new admission
or a fresh collection. Disposable PoC data permits that cutover, not identity reuse.

Add a bounded authority operation equivalent to
`adopt_library_baseline(expected_structural_generation, expected_lock, new_lock)`.
It is separate from user property edits and the structural edit batch. It must:

1. Verify the complete signed candidate closure and expected current generation.
2. Resolve imports and validate every exposed type, used property/slot/reference,
   live occurrence, local connection and stored causal address against it.
3. Compile and validate the full candidate scene, layout, semantics, responders,
   grants and aggregate resource limits without mutating active state.
4. Persist the accepted lock and coherent active closure through the existing
   single-document publication/journal boundary, then acknowledge and notify.

On rejection, disk failure or a stale generation, leave the active lock, scene,
data and receipts unchanged. Restart must recover either the prior complete pin
or the newly accepted complete pin. This is one bounded document operation,
not a transaction spanning arbitrary app documents. Several apps can hold
different exact revisions until independently upgraded.

Compatibility is checked from contracts and actual references, not semantic
version strings. Stable IDs cannot change meaning/type or be reused. Style,
geometry, private composition and new defaults may change if every resident
address and connection still validates and the complete candidate fits bounds.
Deleting an addressed internal node is incompatible even if its name was private.
An additive contract may remain compatible; unknown required features reject.
Do not bump an app's business-state schema merely for compatible library styling.

## Activation, causality and rollback

Connected clients activate only after confirmed edits and finished pointer,
focus/composition sessions that depend on the old baseline. A call/gesture pins
one complete graph, behavior revision and responder set. Never dispatch an old
pointer-down against a replacement pointer-up handler. Local queued intents keep
their original semantic epoch and causal context; stale structural work fails
against the new generation. Compatible independent property intents may continue
after normal address/type/grant validation. Offline unconfirmed RAM edits may be
discarded on upgrade under the existing PoC policy.

Readers pin complete immutable closures. A retired reader never observes new
defaults with an old scene or old handlers with a new graph. Bound retention to
one active, one staged candidate and one retired closure; slow readers must
resync or release their lease before another retirement can be admitted. Do not
retain an unbounded library history in the running host.

Release discovery remains on the stable release-control v1 opening envelope,
independent of state/library decoding. Publish an upgrade event only after the
complete signed code/library replica is active. An unknown library format must
not prevent a client from discovering a compatible host release.

Keep the existing compatible signed code rollback during development. Library
rollback adopts an older verified exact pin through the same validation and
safe-activation path; it preserves data only when still compatible with current
addresses/contracts. An older library that cannot represent current authored data
is rejected rather than silently dropping it. Accepted-work artifact cleanup
continues to follow the existing maintenance policy.

## L1 bounds and integration hooks

Start with a maximum 256 KiB package, 16 KiB canonical graph per library and
32 KiB imported canonical graphs per app. Count resolved imported content against
existing aggregate definition/node/control/scene/layout limits and the 64 KiB
resident authored/causal budget; these are not fresh per-library allowances.
Each admitted closure must fit that budget. Staging and one retained closure add
at most two more bounded source/causal closures; account for their actual memory
and reject allocation before publication. No Frame performance/RSS claim follows
from these source-byte ceilings; measure the first real closure on Frame.

Concrete integration points are:

- Public `oib::graph::Document`: add versioned import/lock/binding contracts;
  preserve canonical app source and stable semantic IDs.
- Bundle admission (`otrios-interfaces::bundle`, authority app admission and
  publication tooling): verify library package kind, exact files and lock closure.
- Core OIB graph compiler: resolve immutable exports before current lowering;
  cache by verified revision plus semantic contract and effective definition hash.
- Core OIB state binding/`configure_oib`: split semantic admission and baseline
  identity, then add the separate expected-generation adoption path and recovery.
- Designer: show imports as read-only library content; edit local instances,
  wrappers and overrides through existing bounded operations.
- Shared client mount/release handling: stage the full pin, quiesce gestures and
  advance coherently without depending on successful old-state decoding.

Fix the exact import-binding codec, canonical hash envelope, compatible-contract
schema and baseline journal record before producing L1 packages.
Do not retrofit an ambiguous digest meaning after libraries are published. D2
need only retain the source/compiled separation, stable addresses, absent-value
inheritance, semantic admission/body separation and explicit changed-baseline
refusal now; imports are not required to finish D2.

## L2 component behavior boundary

Select a separate `otrios-component-behavior-v1` ABI. It is independently
versioned from the existing app behavior ABI 1. A generic host dispatcher admits
typed component context/events and validates typed results; it must not require
a new Rust `ClassKind` or action enum variant for every library widget. Preserve
the base scene/layout/input primitives and library-declared property/slot/outlet
contracts. Do not allow guest-provided Rust/native class implementations.

The initial concrete ABI candidate is one exported memory, `abi_version() -> i32`,
an exported input-buffer offset and `dispatch(ptr, len) -> i64` packing output
offset/length. Inputs and outputs use versioned canonical records, checked memory
ranges and at most 8 KiB each. Freeze export names, bit order, encoding and golden
vectors in the L2 implementation change before publishing executable libraries;
these candidate wire details are not implemented by this RFC.

Input contains the component's stable semantic address, normalized local input
phase/data, effective exposed properties and host-owned local state. The output
contains at most 16 typed commands: update a declared local property, emit a
declared typed outlet, or request/release capture for the component's admitted
target. An outlet is connected to app state only by the app's admitted binding;
the guest cannot choose arbitrary app fields, write a scene tree, issue structural
CRDT edits or acquire capabilities. Validate the whole result before applying
any command. Traps, invalid results and fuel exhaustion leave authoritative state
unchanged and safely cancel an affected capture.

Use Wasmi with no imports/WASI, filesystem or network. Start from the current
64 KiB code, one 64 KiB memory, no tables, recursion depth 32, stack height 1024
and 10,000 fuel-per-call ceilings, subject to real component qualification.
Bound live instances separately (initial maximum eight per app, including staged
replacement stores); reject before allocation when the app exceeds it. A package
signature does not grant additional writes or input ownership.

Host-owned declared local properties are the component's resumable state; guest
linear memory is scratch, not a durable hidden database. Cache immutable modules
by revision but do not share mutable stores between component occurrences.
At a safe activation point, instantiate the new guest from current confirmed
properties; no implicit linear-memory migration is promised. App-owned durable
values still go through existing validated intents and receipts.

Adding this dispatcher is an explicit host update. Subsequent compatible widget
implementations can change as library bytes. A missing ABI/primitive rejects the
candidate and retains the active closure. There is no permissive fallback that
silently substitutes different widget semantics.

[RFC 0026](0026-target-wasm-compilation.md) remains a separate proposed execution
optimization. L1/L2 neither select an AOT engine nor add native executable caches.

## Acceptance and later XR work

L1 acceptance requires an actual signed library-only style/layout revision on
Frame and Web with unchanged host executable/module hashes, unchanged app
collection/semantic epoch and preserved values/overrides. Demonstrate restart
with the accepted pin, coherent pinned snapshots, a held gesture during staging,
compatible rollback, stale-generation rejection and bounded slow-reader resync.
Bad signatures/hashes, missing exports, incompatible types and over-budget
candidates must retain the prior active closure. These are future acceptance
requirements, not results obtained by writing this RFC.

L2 additionally needs cross-host canonical ABI vectors, unauthorized-command and
memory/fuel/trap rejection, capture safety and a new widget whose behavior changes
without a host rebuild. Report native/browser guest execution measurements
separately. Existing in-process checks do not establish Linux IPC behavior.

XR requires a later explicit runtime/backend milestone for spatial transforms,
tracking, display, input/hit testing and permission boundaries. A 2D GLES/WebGL
renderer cannot gain those capabilities solely through a library revision. Once
those stable primitives exist, XR composition, styles and widget behavior can use
the same signed package/pin/ABI model. Do not implement XR, general module
execution, general transactions or portable compilation as part of this loader.
