<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0029: OIB components and live UI data

Date: 2026-11-02. Status: **O1 property editing implemented, validated and deployed**.
The [qualification record](../experiments/live-oib-2026-11-02.md) records browser,
Frame and process evidence. The [OIB guide](../OIB.md) records concrete contracts
and limits. [O2](0030-live-oib-instances.md) subsequently implements bounded
instance creation, duplication and removal. Arbitrary graph placement and
executable class APIs remain design targets beyond these increments.

Builds on [independent UI documents](0013-app-ui-updates.md),
[behavior replacement](0014-app-behavior-updates.md),
[standalone bundles](0025-standalone-app-bundles.md),
[Space-owned data](0027-spaces-and-app-data.md) and the
[shared session](0009-unified-server-session.md).

## The application model

An application's entry point is **its main OIB and main class**. OIB means
**OTriOS Interface Blueprint**: a named, inspectable object graph describing UI,
reusable components, instances, overrides and connections to application code.
The NIB/XIB and prefab analogies describe the authoring experience: construct
objects from data, reuse a definition and connect instances to behavior.

The main OIB constructs the initial interface. The main class receives named
outlets and declared actions and owns application behavior. Schema, assets and
dependencies are reachable from these entry points and the signed package's
existing metadata. Identity, integrity and access policy remain required; two
entry points do not replace those contracts.

UI construction becomes an application of synchronized data, rather than a
rerun of Rust authoring functions. The existing layout, scene, retained
presentation and input runtimes continue to execute the resolved description.
Computed geometry, GPU resources, clocks and pointer captures remain local.

## Ownership and scope

Application ownership means ownership of the UI schema, component definitions,
class contracts and allowed mutations. As in RFC 0027, the Space owns the
persisted instance data, including its live UI changes. The host authority
stores, validates and synchronizes that data on the Space's behalf.

| Object | Owner and update path |
| --- | --- |
| Signed OIB seed and main-class code | App publisher; immutable verified bundle |
| Live component definitions, instances and overrides | App's UI namespace in the Space-bound collection; authority-confirmed edits |
| Ordinary app content | Existing app data schema and operations in that collection |
| Permission to edit UI | Authority policy intersected with the signed app's declared UI editing surface |
| Selection, inspector, drag preview, focus and animation | Local session; never the shared OIB |

One `(Space, app, collection)` binding has one live UI document initially. An
edit affects participants using that binding, including native and browser
clients. It does not silently change another Space or the installed app bundle.
Changing the publisher's default UI is a separate explicit signed publication.

For O1, Space Admins can use the declared UI editor. Members and visitors receive
the active UI but cannot mutate it. Application code may request only declared
UI operations in its admitted binding and cannot elevate the participant's
authority. A local trusted management client still uses the existing host policy;
there is no new identity or trust root. Later apps may declare narrower Member
editing surfaces through a separately reviewed policy extension.

The installed bundle advertises a required OIB feature and an immutable edit
schema. Mutable OIB data cannot add classes, code hashes, schema fields, effects,
write permissions, services or cross-Space references. Bindings and actions must
resolve to the signed application's already admitted declarations.

## Stable objects and reusable components

Use distinct stable identities for OIB documents, component definitions, template
nodes, instances, exposed properties, slots, outlets and actions. Names such as
`LabeledSlider` and `volume` are human-readable lookup names; renaming a name
does not replace its identity. Duplicate names in one lookup scope are invalid.
Seed objects retain their signed nonzero u64 IDs. The authority reserves those
IDs, allocates IDs for new objects and never reuses deleted IDs within the
collection's lifetime. Offline creation is deferred.

These identities are distinct from the current compiler-generated u16 layout
and control indices. A resolver produces compact indices for each activation
and retains the mapping from stable OIB identities. Events carry the active
mount/scene generation, so a stale event cannot target a newly reused index.

The logical model is:

```text
ApplicationEntry { main_oib, main_class }
OibDocument      { id, schema, root_instance, definitions, instances }
Component        { id, name, root_node, properties, slots, outlets, actions }
TemplateNode     { id, type, properties, placement, children }
Instance         { id, name, component, placement, overrides, slot_children }
Override         { instance, template_node, property, value }
Connection       { source_object, declared_outlet_or_action, admitted_target }
```

This is a logical schema, not an encoding of Rust structs or a wire ABI.
Component dependencies must be acyclic. Each instantiated object has exactly
one parent. Nested instances retain their own IDs; the resolved identity of a
template node is its instance ancestry followed by its stable template-node ID.
Changing a template's shape does not renumber surviving nodes.

A component exposes typed properties and named child slots. For example,
`LabeledSlider` exposes `label`, `accent` and a numeric `value` binding. A screen
places `volume` and `brightness` instances of it, each with its own binding and
label override. Changing the definition's accent updates both instances unless
an instance explicitly overrides accent. Changing `volume.label` changes only
that instance. An explicit **Reset override** removes that override and resumes
inheritance; setting a value equal to the default is still an override.

For example, an author-facing representation could be:

```text
component LabeledSlider {
  label: Text = "Level"
  accent: Color = #DAA520
  value: Binding<Number>
}
main {
  volume: LabeledSlider { label = "Volume", value <- app.volume }
  brightness: LabeledSlider { label = "Brightness", value <- app.brightness }
}
```

`SetProperty(LabeledSlider, accent, #5588FF)` changes both controls live;
`SetOverride(volume, accent, #FF8855)` then changes only the volume control.
The notation illustrates the data model; O1 does not introduce a text compiler.

Resolution proceeds from component defaults to instance property/slot overrides
and then evaluates the resulting declared bindings against app content. A bound
property has one source expression; a reducer cannot silently compete with that
binding by writing a rendered value. Code must change the binding's app data or
explicitly request an allowed OIB override. Focus/pressed/animation decoration
is a local presentation layer and never writes back into the OIB.

OIB v1 has composition and per-instance overrides. It has no component subclass
inheritance, arbitrary scripts inside property expressions or implicit deep
copies of templates. Named variants can be considered after these rules work.

## Instantiation from UI and code

A UI editor and application code must use the same typed mutation API. Proposed
operations are `Instantiate`, `SetProperty`, `SetOverride`, `ClearOverride`,
`MoveInstance`, `Connect` and `DeleteInstance`. They address stable identities,
not array positions, display names or generated responder numbers.

For example, both dragging `LabeledSlider` into a screen and requesting
`ui.instantiate("LabeledSlider", parent, properties)` create an instance of the
same admitted definition. A shared instance is created once by an authority
operation and replicated; it is not independently created on every client's
render pass. Give each request an idempotency identity so retries do not duplicate
instances. A local transient instance must be explicitly requested as local and
does not enter the synchronized graph.

The main class is a stable declaration resolving to already verified bounded
behavior. An outlet is a typed reference, and an action is a declared event
contract, not a string used to invoke arbitrary code. A declarative-only app can
use the runtime's no-op main class. OIB activation reconnects surviving outlets
without rerunning application startup or resetting the class instance. Deleted
outlets become absent; undeclared or incompatible connections fail validation.

This does not introduce native object loading, a general module system or a new
Wasm execution backend. The current reducer ABI has no general object/class or
UI mutation imports. O1 uses the existing declarative event interpreter unchanged;
adding the proposed
class lifecycle and code-instantiation calls requires the later bounded ABI work.

## CRDT representation and conflicts

Do not replicate an entire `ui.flex` blob for every change or place the graph in
one of the 64 ordinary app fields. Extend the collection's authority-owned
aggregate with an OIB namespace and reuse canonical Merkle nodes, causal
registers, durable receipts and covered subscriptions. The current Canvas state
adapter expects resolved registers; an explicit OIB adapter is required.

Each editable property has its own typed causal register. An override cell has
three meanings: no assignment inherits; an assigned value overrides; a causal
clear resumes inheritance while retaining deletion metadata. Deleting an object
tombstones its identity. Edits cannot resurrect a tombstoned instance; explicit
recreation receives a new ID.

Operations carry the causal context actually observed for their target cells.
The authority must not replace a stale writer's context with its newest context,
which would silently erase concurrency. Disjoint property changes merge. Two
different concurrent values for the same property remain a multi-value conflict,
including a concurrent value and clear. Equal typed assignments may render as
one value but retain all causal metadata. No wall-clock last-writer rule is used.
A resolution explicitly observes and supersedes all conflicting alternatives.
Resetting a definition property writes its signed seed default; resetting an
instance property causally clears its override. Undo is a new scoped edit,
never restoration of an old whole-document snapshot over other people's work.

The desired structural model uses one atomic placement register per instance:
`{ parent, slot, order_key }`. Moving an instance therefore cannot create two
independently writable parent edges. Siblings sort by order key then stable ID;
concurrent reparenting conflicts. Component-reference edits and delete/edit races
also require explicit conflict handling. A graph validator checks cycles,
missing objects, deleted targets, root uniqueness and expansion limits. A cycle
introduced by merging two individually valid moves is a draft error, never a
partially rendered tree. General sequence CRDTs and structural editing are not O1.

## Draft data and the active view

Separate synchronized **draft CRDT state** from the **active validated OIB**.
This permits faithful CRDT merging without asking a renderer to display an
invalid graph. The authority publishes both in one collection aggregate:

```text
OibState {
  seed_bundle_digest,
  edit_schema,
  draft_root,
  active { generation, resolved_oib_digest, source_draft_root },
  issues
}
```

A permitted edit is merged into the draft and durably receipted. If the complete
draft has no unresolved value conflicts or graph errors, the authority resolves
and validates its complete bounded UI closure, then advances `active`. If the
draft cannot activate, `active` remains unchanged and editors receive bounded
issues tied to the affected stable objects. The result distinguishes **applied
and active**, **saved in draft with issues**, and **rejected**. Unauthorized,
malformed, wrong-schema or over-budget submissions are rejected before admission.

Only the authority advances the active pointer. New clients and existing clients
read the same active revision, even when a draft has conflicts. Clients do not
independently choose a register winner or persist a private last-good UI as the
shared result. Ordinary viewers subscribe to active UI and required app state;
editors additionally subscribe to the bounded draft. A local preview is marked
unconfirmed and cannot dispatch actions through an unadmitted description.

One bounded edit batch belongs to this aggregate only. Persist the draft, issues,
any new active bytes and the receipt before acknowledgment; publish the complete
root only after its referenced content is available. Recovery must restore that
same relationship. This is not a general cross-collection transaction API.
Within the existing bounded receipt window, identical requests replay their
existing result; reusing an identity with different bytes fails. Outside that
window an uncertain outcome requires resync, not blind replay. Slow clients also
receive explicit resync when their bounded cursor cannot continue.

The active resolved OIB is a content-addressed property graph, so participants
can receive changed active cells without downloading an entire compiled UI blob
per edit. `resolved_oib_digest` identifies that graph's root. The compiled layout
and scene are disposable derived caches keyed by the seed, edit schema, resolver
version and source draft hash. Pin the complete active closure until activation
finishes; reproduce it deterministically on recovery.
SHA-256 covers canonical bytes including causal metadata where appropriate.
Hashes identify bytes, never grant editing permission or publisher authenticity.

## Live edit and presentation path

1. An authorized editor selects an instance or opens its component definition.
   The inspector shows inherited values and explicit overrides.
2. Inspector changes submit a small typed batch with request identity and observed
   causal context. Drag preview stays local; coalesce unissued updates to a
   bounded cadence and always submit the final value on release.
3. The host owning the collection admits, merges and persists the batch, resolves
   a valid candidate and publishes the new aggregate root on the existing session.
4. Native and browser clients receive changed active OIB paths, verify the graph
   and resolve it through the shared runtime, staging its scene and semantic/input
   mappings together.
   New selector dependencies obtain compatible state coverage before activation.
5. At a presentation boundary, activate the complete candidate. Reuse the main
   class, compatible app content, retained resources and surviving local editors.
   A successful UI edit requires no build, package signing, reload, WebSocket
   reconnect or behavior replacement.

Paint-only changes may present during input. Geometry and connection changes
follow the existing captured-scene rules; they must not retarget a held pointer.
For O1, defer layout activation until gestures finish and connected app edits are
confirmed, coalescing pending active revisions. Preserve focus by stable object
identity and compatible control type. Removing/rebinding a responder in later
structural work must cancel its capture before swapping generations. Modal
barriers, ancestor bubbling, explicit decorative pass-through and clickable
clipped regions remain the shared runtime's responsibility.

“Realtime” means event-driven propagation and the next eligible presentation,
not a hard latency promise. Measure edit-to-confirm and confirm-to-present on the
live Frame and browser. Network delay, unfinished gestures or pending confirmed
edits can delay activation; expose pending state in the editor when useful.

Use the existing shared WebSocket and preserve release-control v1. The OIB wire
extension requires a bump from the then-current state protocol, with matching
browser/native/server codecs and explicit feature negotiation. Reject OIB/state
bytes before compatible negotiation. A client with an old codec must still
discover the current signed runtime through its opening hello/upgrade path.
No second socket or state-decoding prerequisite is added to release discovery.

## Signed seeds, updates and recovery

The signed bundle supplies the OIB seed, main-class declaration and immutable
edit schema. Proposed bundle metadata names `main_oib` and `main_class` under a
new required package feature; this is not an optional key old hosts may ignore.
O1 retains `Info.flex.entry_ui = UI/main.flex`: Canvas v8's signed
`Presentation::Oib(Seed)` carries these two declarations, with the required
`otrios-oib-v1` feature guarding admission. Separate executable class metadata
remains future work. Continue to use the existing signature verifier, asset
admission and bounded Wasm runtime.

Live UI is authorized Space data, stored outside the bundle. A signed package
authenticates the seed and the allowed execution surface; it does not sign every
subsequent participant edit. Peers receive that live state through the admitted
authority session. UI editors cannot replace verified executable bytes through
CRDT fields, and code sharing still exports only the immutable signed package.

Pin each live document to its seed digest and edit-schema identity. A package
update may retain the document only when its required contracts and IDs remain
compatible. O1 provides no automatic rebase onto a changed seed. An incompatible
update reports the UI unavailable rather than clearing overrides or silently
resetting data. Resetting the UI to its installed seed is an explicit new edit.

Before rollout, retain a verified code rollback that understands the new OIB
state. Switching code must not roll the collection back to old data. A binary
that cannot read the OIB extension fails explicitly. Keep initial qualification
isolated to its test app; existing app mounts continue using their admitted UI
format until an explicit cutover.

UI draft and active state follow collection retain/remove/delete/move semantics.
Portable data export/replacement must version and include the confirmed OIB
namespace, seed/schema binding and conflict metadata before general app adoption.
Do not silently omit layout edits from a supposedly complete data export. Public
viewers receive active presentation data only, not inspector drafts or management
records. Revocation clears private OIB bytes and drafts with the private mount.

## O1: first implementation and visible edit

Implement a bounded `Live UI` companion app linked from Test Playground. Attach
it to the same Space through the existing Admin operation; it has its own data
collection, as the current separately attached demos do. It has a predeclared `LabeledSlider` definition
and two stable instances, `volume` and `brightness`, bound to its own volume and
brightness numeric fields through existing declarative operations. Its main
class is `otrios.declarative-v1`; this milestone does not implement guest imports.

The initial inspector permits typed label, accent, preferred width and spacing
edits, plus setting/clearing the corresponding instance overrides. It names the
editing scope: **Component: LabeledSlider** or **Instance: volume**. Use a
scrolling inspector panel. Definition edits affect both instances; instance
overrides demonstrate independent customization and reset to inheritance.
There is no node creation/deletion, reparenting, connection editing or class-code
replacement in O1. Those operations above specify the intended model for later
milestones rather than hidden work in the first delivery.

Proposed O1 admission limits: 16 component definitions, 64 authored objects,
64 editable cells, 16 changed cells per operation and at most 64 KiB of canonical
OIB draft data including conflict metadata. Also enforce the existing lower
applicable runtime limits: 128 KiB compiled layout, 512 expanded scene nodes,
depth 16, control/index and coordinate limits, and 512 KiB Canvas messages.
Budget the entire expanded closure, including repeated instances; a small
template is not permission for unbounded expansion. Exhaustion rejects edits
without advancing active UI. This milestone adds no dependencies by default.
Retain existing per-register actor/context limits and writer-receipt bounds;
report exhaustion explicitly. O1 adds no history garbage collector and must not
discard causal context to admit another edit. Validate all retained conflict
values, not only a candidate selected for display.

Use a separate OIB aggregate branch with fixed metadata and a cell trie. One
possible bounded path allocation is `[OIB, cells, <13 base-32 digits of CellId>,
leaf_kind]`: depth 16, fanout at most 32. Immutable cell descriptors bind an ID
to definition/instance, property and type; value leaves carry causal registers.
The authority rejects duplicate target cells and descriptor changes. Coverage
can select the OIB branch without one path per cell. This layout must be pinned
with canonical bytes and vectors before coding the adapter; it is not an
existing encoding. The first milestone's fixed cell table avoids introducing
dynamic map/sequence CRDTs or consuming ordinary app field IDs.

Required evidence for O1:

- Open the same attachment in two browser clients and on the physical Frame.
  Edit component accent: both slider instances update on every participant.
  Override `volume.label`: only that instance changes. Clear an override:
  inheritance resumes, including after reconnect and authority restart.
- Change preferred width/spacing while a slider is held. The gesture keeps its
  original target, the pending layout activates when eligible, and its numeric
  app value survives. Focus, keyboard, class instance, page/worker/session and
  native renderer ownership survive compatible edits.
- Submit concurrent disjoint edits: both survive. Submit differing values to
  one cell from the same observed context: both are retained as a draft conflict,
  every viewer keeps the same active revision, and a resolving edit activates on
  every participant. Test duplicate, reordered and repeated CRDT merges, including
  clear/value concurrency, against identical canonical hashes.
- Use process-boundary authority tests for durable receipts, unknown outcomes,
  restart, stale epochs and slow-reader resync. Check inactive-screen edits,
  private revocation, cross-Space isolation and explicit refusal of incompatible
  portable export/replacement during the isolated experiment.
- Reject unauthorized edits, forged cell descriptors, action/code changes,
  nonfinite geometry, invalid types, oversized conflicts and expanded scene
  exhaustion while leaving the active scene usable. Preserve stable opening
  hello behavior when an older state client reconnects.
- Deploy the signed runtime once through the existing verified release workflow,
  then perform the UI edits using data alone. Record release hashes, unchanged
  code/process/session identities, propagation timings and a compatible signed
  rollback. These are live acceptance requirements, not claims made by this RFC.

Runtime resolution, validation, merge handling and tests belong in
`crates/otrios-core` under LGPL-3.0-only. Contracts belong in
`crates/otrios-interfaces`, reusable client declarations in `crates/otrios-sdk`,
and app authoring/tooling under their existing Apache-2.0 boundaries.

## Later increments and retirement

After O1, separately scope graph instantiation/deletion and placement conflict
handling; the main-class/outlet lifecycle and bounded code UI API; complete
portable transfers; compatible seed evolution; and conversion of existing apps.
Each increment needs its own bounded acceptance before widening live adoption.

Rust UI builders can remain seed-authoring tools during conversion. For an
OIB-enabled mount, the synchronized graph becomes its only live UI owner; do not
also regenerate its mutable UI from `ui.rs` on each edit. Existing package UI
updates remain available for legacy mounts and explicit seed publications.
Retire generated indices as persistent editing identities, not as compact scene
implementation details. Keep the shared layout/compositor, authority, transport,
retained presentation, package verifier and release-control implementation.
