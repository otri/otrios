<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0003: Module roots, typed selectors and scoped subscriptions

Status: proposed design, extending [RFC 0002](0002-incremental-crdt-hierarchy.md).
The live dashboard still has one shared Automerge room, enum-based bindings,
bundled screen layouts and named navigation actions. This RFC defines the next
contract. An isolated [RFC 0004 prototype](0004-canvas-behavior-and-app-packages.md)
now mounts Home, Playground and Layout Lab with independent Automerge documents,
flat typed field selectors and optional Wasm reducers. General collection
selectors and incremental Merkle replication below remain proposed.

## Decision proposed

Use schema-checked, binary selector plans rooted in explicit module inputs.
Separate singular read/write focuses from read-only collection queries. Bind
editable collection members by stable identity, not numeric index. Keep
replication subscriptions and UI dependency invalidation distinct.

This borrows collection projection from KVC, composable focuses from lenses and
traversals, and explicit collection selection from JSONPath. It does not adopt a
CSS grammar, dynamic Objective-C dispatch, JSON transport or the full JSONPath
query language. Readable examples below are authoring/debug notation only.

## A module owns an explicit UI/state boundary

An activated module instance receives exactly two logical subscription roots:

```text
Canvas(uiRef = playgroundUiRef, stateRef = playgroundStateRef)
Canvas(uiRef = layoutUiRef,     stateRef = layoutStateRef)
```

Each root is a logical ObjectRef whose current revision is a Merkle ContentId.
Subscriptions follow logical identity; immutable revision hashes identify the
bytes of a pinned snapshot. Binding directly to a revision hash would freeze the
binding. Two module instances can share UI bytes while taking different State
references, or intentionally share the same State reference.

The Playground subscribes to its UI and State roots only. The runtime fetches
necessary descendants of those roots and immutable component/assets as needed;
these are not subscriptions to unrelated modules. Cached libraries/templates do
not acquire ambient access to application state. Global theme/shared-state
inputs, if needed later, must be explicitly declared additional inputs, not
hidden global dependencies.

A navigation action in Playground UI carries opaque destination references:

```text
Navigate(ui = layoutUiRef, state = layoutStateRef, parameters = ...)
```

It need not inspect, download or subscribe to either destination first. Navigation
is handled by the shell's responder chain. On activation the shell authorizes,
resolves, subscribes and qualifies the destination UI/state pair; then switches
the local view and releases the old module's leases. A temporary overlap is
allowed during preparation. Failure retains the old usable module. Back-stack
entries retain references/route parameters, not live subscriptions or durable
client replicas. Back resubscribes. Destination identities are not credentials.

## UI and bindings are data

UI trees contain stable node IDs, primitive/template references, properties,
bindings, local layout expressions, semantics, responders and navigation data.
The rendering host provides primitive behavior and a bounded interpreter.
Layout/instance edits become data revisions; adding a new primitive or executable
behavior still follows the signed-code boundary.

One selector can feed several UI properties:

```text
let linear = $state.controls.linear.value          // Ratio in 0..1
Slider.value       <- linear
Slider.onDrag      -> Set(linear, event.value)
Percentage.text    <- Percent(linear, precision=0)
```

`Percent` is a typed read-only projection. It does not create a second durable
percentage field. The slider and label share a compiled selector/dependency
entry. Their value, text, semantic label and responder configuration advance in
one local presentation snapshot, avoiding mixed revisions.

A binding declares its input schema/version, result type, cardinality, allowed
missing-value behavior, and access mode. Field names are authoring conveniences;
validated plans use stable schema field IDs, typed map keys and explicit item
identities. Field IDs cannot silently be reused across schema versions. Units
and numeric conversions are explicit; current 0..1000 control registers need an
adapter to the example Ratio type.

## Selector categories and collection semantics

| Example notation | Result | Dependency and edit meaning |
| --- | --- | --- |
| `$state.controls.linear.value` | One Ratio | Field path; writable only with an explicit Set capability |
| `$state.items.byId(itemId).title` | Optional Text focus | Stable logical item plus membership guard; editable while present |
| `$state.samples.at(3)` | Optional Number | Fourth visible position; re-evaluate on relevant order/membership changes |
| `$state.items.each().title` | Ordered keyed results | Membership/order plus selected fields; preserves item IDs |
| `$state.items.where(.visible).each().title` | Ordered keyed results | Membership/order, visibility of candidates, title of matches |
| `$state.items.count()` | Integer | Membership/count metadata; unrelated title edits do not matter |
| `$state.items.window(cursor, 50)` | Bounded keyed page | Stable cursor anchor, membership/order and the selected page |

An array index means a position in a pinned visible sequence, never permanent
item identity. A repeated row receives an ItemRef; its internal selectors are
relative to that item. Moving an item preserves logical ItemId even when the
chosen sequence CRDT represents the move with new placement metadata. Logical
ItemId and CRDT placement/operation ID must not be conflated. The keyed-sequence
CRDT's concurrent move, duplicate membership and delete rules need explicit
conformance vectors before implementation.

For an edit originating from a positional view, resolve the displayed position
to ItemId when creating the responder target. Carry that identity and relevant
membership/schema preconditions with the command. A later insertion/reorder
must not retarget it. A removed target yields Missing/StaleTarget, not a neighbor.
A direct object reference may outlive collection membership, but a row binding
must retain the membership guard. Do not reuse deleted identities.

Text edits need stable CRDT cursor/range anchors with explicit affinity, not
numeric UTF-8/grapheme offsets masquerading as permanent identity. Text editing
is a separate typed operation family, not bulk assignment through array paths.

Selectors return Present, Missing, NotLoaded, TypeMismatch or Forbidden as
explicit states. Missing is distinct from null, false or zero. Schema checks
normally reject TypeMismatch before activation. UI declares a placeholder or
hides/disables a missing target; no implicit object creation occurs on write.

## Singular writes and read-only derived queries

A singular schema field focus is lens-like; an optional member focus can resolve
to no item. A traversal/query produces zero or more keyed results. In version 1,
queries and derived projections are read-only. A query returning one item today
does not automatically become a safe write address. A rendered item resolves a
singular identity-based target for any action.

The command layer selects CRDT operations by field kind: Set register, Increment
counter, Insert/Delete sequence item, or text edit, with target identity and
preconditions. A generic Set must not overwrite a counter/sequence's causal
metadata. Writes are validated by the authority; selectors alone never grant
access. Optimistic local overlays and continuous-input coalescing remain separate
from durable state. Distributed acknowledgement/merge semantics do not inherit
ordinary synchronous lens laws by analogy.

No implicit inverse is invented for Percent, sums, filters or sorts. Bulk editing
requires a separately declared operation and target set; general transactions
remain outside this milestone.

## Two layers of subscription

1. **Replication:** subscribe to authorized logical UI/State roots and request
   only missing revisions/deltas within the necessary subtree coverage. A State
   lease may request the full module subtree initially, then a selector-derived
   coverage set as partial materialization is implemented. Either way it never
   includes Layout Lab history just because the global catalog root changed.
2. **Presentation:** compiled selectors track precisely which value/structure
   results affect which UI properties. A module State root changing is a reason
   to inspect its changed paths, not repaint every descendant UI node.

Merkle identity covers causal merge metadata. A new merge state can change its
hash without changing the resolved value. Replicas must accept the metadata
update, while presentation can retain its geometry when the selector result is
unchanged. Conversely a sequence reorder changes a collection result even if
every member's value hash stays equal. A keyed result therefore includes IDs,
order and relevant values. Equality of an individual projected scalar can still
suppress repaint of a sibling text label.

The authority routes root changes by subscription identity, then the selector
engine intersects changed paths with a dependency index. Exact field watches,
collection membership/order watches and bounded query predicate watches are
separate. Do not execute every selector or walk every app tree per edit.
Intern identical selector plans within a module activation; one selected value
can invalidate multiple consumers without duplicate network subscriptions.

Path resolution also watches traversed parent links and absent lookup slots,
not only the last leaf. Replacing `controls.linear`, removing an intermediate
object, or creating a previously missing field must re-resolve its descendants.
A binding cannot remain attached to an obsolete leaf just because that leaf's
hash did not change. Resolving and registering this dependency set uses the same
snapshot/publication boundary as subscription setup.

Filters watch predicate inputs for *all candidate members*, including currently
excluded items; otherwise an item that becomes visible would never enter the
result. New members are also candidates. Unindexed filters have O(candidate
count) evaluation/coverage cost, not O(matches). Range/order queries need
structure watches, not just watches on currently visible leaves. Start with
bounded scans, then introduce explicit incremental indexes/aggregates when
measured. No recursive descent or arbitrary callbacks in the first interpreter.
Predicate, path depth, result size and execution/allocation budgets are explicit
contract limits, to be selected and tested before code activation. Never silently
truncate results when a budget is exceeded.

## Coherent publication, cancellation and policy

Capture the initial snapshot and register watches under the same authority
publication boundary; changes between fetch and subscribe cannot be lost.
Updates carry authority epoch, logical root, revision/cursor and subscription
identity. View notifications may coalesce to latest complete results. Skipped
coverage or cursor gaps require explicit resynchronization; required CRDT deltas
must not be dropped merely because the UI skips intermediate frames.

Each local module activation has a new scope/lease generation. Late messages for
an exited scope are ignored, even if the same module is reopened with different
State. Navigation, expiry and revocation cancel leases and pending old input.
Immutable cached UI code/data can remain under the artifact budget. Private view
and working-state lifetime stays RAM-only under the existing policy.

UI revisions declare compatible State schemas. Prepare subscriptions/coverage
needed by a replacement UI before activation, validate bindings, then publish a
coherent local UI/state view. A compatible UI-only edit need not wait for an
unrelated State edit. For a coordinated schema migration the PoC authority must
publish an explicit compatible pair/generation or require reactivation; two
independent subscriptions are not a cross-authority atomic transaction. Opaque
navigation references and known content hashes do not authorize retrieval of
Monitoring or other private subtrees. Revocation is a control event even when
no data hash changes.

## Initial binary plan and migration slice

The planned data contract has a root slot (`State` or item-local context), schema
identity/version, typed path steps, cardinality, result type, read/write mode,
explicit projections/predicates and a missing-value policy. A navigation target
has UI ObjectRef, State ObjectRef and typed parameters. A compiled binding links
its selector plan to a UI node/property and optional semantic write action.
Use the existing versioned FlexBuffers contract approach; binary tags alone do
not supply schema/permissions. This RFC does not reserve stable wire tags yet.

Implement this as a new version alongside `otrios-layout-v1`; do not silently
reinterpret current Binding::Local or numeric control IDs. First slice:

1. Give Playground and Layout Lab separate UI and State logical roots. Separate
   Automerge documents can establish the module isolation boundary before a
   replacement CRDT representation is ready. Merely hiding fields in a renderer
   while syncing the whole room does not pass isolation.
2. Add Field/typed MapKey/ById selectors and explicit singular register writes.
   Bind the Playground slider, value text and accessibility value to one field;
   navigate through opaque UI/State references. Test two instances sharing UI but
   using different State roots.
3. Add keyed Each/Count and a positional read view with identity-based edit
   targets; then bounded predicates and cursor windows with dependency tests.
4. Introduce the RFC 0002 immutable hierarchy behind the same selector/module
   contracts and compare delta bytes, changed nodes and view rebuilds on the
   frame/Wasm. Keep raw hash cost separate from full merge/publication latency.

## Acceptance cases

- One slider edit changes both bound properties in one view and one semantic
  state mutation; label/semantics do not maintain duplicate state fields.
- Zero Layout Lab payloads, invalidations or CRDT history while Playground is
  active; navigation fetches the target on demand and unsubscribes on leave.
- Sibling field, inactive module and unrelated catalog changes produce no
  presentation invalidations for a selector that did not change.
- Metadata-only merge syncs correctly with no value repaint. Reorder invalidates
  ordered collections; ById still addresses the same member.
- Insert before an edited row cannot redirect the write; concurrent deletion
  gives StaleTarget, including with a recycled screen/late pointer event.
- An excluded item changing its predicate becomes included; a nonpredicate field
  change on an excluded member does not repaint the filtered result.
- Same module/UI with two State inputs has no ambient-global cross-talk. Old
  activation messages cannot affect either a new State input or a reopened view.
- Snapshot/watch registration races, cursor gaps, slow-reader coalescing,
  missing/not-loaded nodes, denied references and incompatible schema pairs
  have explicit outcomes and leave the prior valid view intact.
- Measure node/hash work, selector executions, allocations, bytes delivered and
  geometry rebuilds as inactive module size and collection size increase. Indexed
  lookup and bounded queries must be distinguished from scans in reports.

These are future acceptance tests, not claims about the deployed dashboard.

## Design references

Apple's [KVC object/key-path guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/KeyValueCoding/BasicPrinciples.html)
describes property paths and projection through to-many relationships; its
[collection operators](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/KeyValueCoding/CollectionOperators.html)
provide aggregate operations. KVC supplies useful authoring ideas but not this
proposal's CRDT identity, access or incremental-subscription contract.

The [lens](https://hackage.haskell.org/package/lens/docs/Control-Lens-Lens.html)
and [traversal](https://hackage.haskell.org/package/lens/docs/Control-Lens-Traversal.html)
references motivate composable single/multiple focuses. This design adds
optional targets and asynchronous semantic commands rather than importing their
functional implementation or promising distributed lens laws.

[JSONPath RFC 9535](https://www.rfc-editor.org/rfc/rfc9535.html) specifies array
index, slice, wildcard and filter selection. It is useful comparison material;
JSON value queries do not define this proposal's CRDT-aware stable references,
writes or watch lifecycle.
