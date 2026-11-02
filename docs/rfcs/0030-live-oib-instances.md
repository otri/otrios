<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0030: Bounded live OIB instances

Date: 2026-11-03. Status: **O2 implemented and deployed**.
See the [live qualification](../experiments/live-oib-instances-2026-11-03.md).
Extends [OIB O1](0029-live-oib-ui.md) with creation, duplication and removal in
the existing isolated Live UI app. Executable class/outlet APIs, arbitrary graph
placement, mutable bindings, seed rebasing and portable OIB transfers remain
separate increments.

## Scope and ownership

A signed OIB instance surface declares factories by identifying seed preview and
inspector subtrees. Factories reuse only the component's admitted properties,
data bindings, numeric handlers and inspector actions. They cannot introduce
code, schema fields, arbitrary operations or cross-Space references.

The shared operations are Instantiate(component, optional name),
Duplicate(instance, optional name) and Remove(instance, observed presence).
Both editor controls and typed Client/StateAdapter APIs issue the same requests;
Wasm host exports expose the same methods. This is a host API, not a new guest
Wasm import or main-class execution ABI. Existing Space Admin admission applies
to every shared operation independently of visible controls.

New instances use a signed seed prototype's existing binding. Duplicates copy the
source's active overrides at authority acceptance and retain its prototype and
binding. Clearing an override resumes inheritance. Adding a LabeledSlider uses
Volume; duplicating brightness retains Brightness. The editor names the binding.
No new app-content field is allocated, and app values survive structural edits.

## Identity and causality

The authority appends immutable Allocation records (id, name, component,
prototype) to a bounded ledger. Seed IDs remain reserved. New IDs start at
0x10000 + allocation index and never reuse a removed allocation. Names use 1–64
ASCII letters, digits, hyphens or underscores. Automatic names include the
monotonic allocation index. Concurrent explicit names may conflict; duplicate
live names retain the previous active graph until one allocation is removed.

Each allocation has a causal draft presence register. Remove observes that
register; removal wins over simultaneous property changes, so a stale property
edit cannot resurrect an instance. Property registers remain separate, preserving
O1 disjoint-edit and same-property conflict behavior. Removed descriptors,
properties and causal context stay retained. There is no history garbage collector.

A request uses the existing writer sequence/digest receipt. Identical retries,
including after an unknown outcome and authority restart, do not allocate twice.
Changing the bytes of a retained request identity is rejected. Authority
allocation is serialized; this is not offline ID generation.

## State and activation

The OIB branch retains O1 metadata [4,0], draft properties [4,1], active properties
[4,2] and status [4,3]. O2 adds the immutable append-only allocation ledger at
[4,4], draft presence trie [4,5] and active presence trie [4,6]. Tries use the
existing 13-digit base-32 u64 path encoding. Schema hashes distinguish instance
presence from property cells. New property cell identities are derived from the
retained allocation index and signed component-property mapping.

Only a valid complete draft advances active generation. A new allocation enters
only the draft until validation succeeds. Invalid names, geometry, conflicts or
expanded closure exhaustion preserve the exact active root and scene. Editors
can remove a pending allocation through the same causal Remove operation. The
active root hashes active property and presence roots with a domain separator;
status pins the corresponding draft roots. The existing durable journal and
receipt publication cover the entire operation.

Editors receive the draft. Other participants receive immutable seed/allocation
metadata, active properties, active presence and bounded status, without draft
property values or causal presence intent. Allocation metadata, including names
of pending allocations, is shared bookkeeping. The renderer uses only active
presence. This remains an isolated UI experiment; OIB portable export/replacement
continues to fail explicitly instead of omitting UI state.

Graph compilation is deterministic. Signed prototype node-rank tables reserve
compact layout IDs for each lifetime allocation. Controls and selectors have
stable allocation-based mappings; deleting another instance never renumbers a
surviving target, and deleted responder IDs are not reused. Expansion preserves
the existing 128 KiB layout, 512 scene-node, control-count, selector and coordinate
bounds. Derived layout v5 permits sparse responder IDs 0–254 while reserving 255
for the host; earlier layout versions retain their 0–127 event-index bound.
The larger identity namespace retains deleted controls without increasing live
control count. A bundle authored directly with layout v5 must declare that
required feature; O2-derived layouts are guarded by the O2 package feature. Unsupported factory shapes/actions and targets outside a declared
prototype are rejected at admission.

O2 admits at most six live instances and ten lifetime allocations, including
the two seed instances. The fixed 64-cell OIB budget and combined 64 KiB draft
budget also apply. Lower compiled-closure limits may reject activation. Exhaustion
is explicit; it never discards tombstones or causal context to make room.

## Local interaction and performance

Every client keeps its own inspected instance and independent preview/inspector
scroll offsets. One inspector is expanded for the locally selected instance;
expanding one full inspector per object exceeds the existing layout-byte limit.
The preview is a clipped scrolling viewport. Tap an instance heading to inspect
it; pan its noninteractive background or use a wheel over that viewport to reach
additional instances. Slider captures retain their own target.

Shared activation waits for held slider or panel gestures and unconfirmed app
intents. It preserves mount/epoch, app state, focus and renderer ownership. Local
inspection does not write shared data. Scroll frames update retained placement
and clipping instead of rebuilding geometry. Visible portions of clipped controls
remain interactive.

## Compatibility and rollout

State protocol is v22; tag 38 carries one canonical bounded InstanceOperation.
Property tag 37 is retained. Browser, native/server codecs and publisher metadata
must agree. The stable release-control v1 opening envelope and upgrade discovery
remain independent of state decoding on the existing shared WebSocket.

Bundles with an instance surface require `otrios-oib-instances-v1` as well as
`otrios-oib-v1`. The signed surface explicitly allows one upgrade from the exact
previous UI SHA-256. It retains main OIB/class, component/instance identities,
property identities, inheritance, types, defaults and choices. Admitted node
mappings and presentation may change. Existing property values and conflicts
survive; unrelated seed changes remain rejected. This is not general seed rebasing.

Before publication, validate the exact deployed O1 UI fixture, journal replay,
concurrent changes and old-control identities. Deploy through the signed-release
workflow, retain a v22/O2-compatible recovery kit, and qualify the same attachment
on browser and Frame. A v21/O1 binary is not a compatible rollback after O2 state
is installed. Code restoration must not restore old collection data.

Required acceptance: create a third instance through the UI and typed API;
change its override and a component property; duplicate/remove; observe on another
client; retain local inspection/scroll and held-gesture behavior; reject Member
writes; recover unknown outcomes and removals across real authority processes;
record live release/process identities and bounded expansion. Runtime logic and
tests stay LGPL-3.0-only; contracts, authoring, SDK exports and tooling retain
Apache-2.0. No new dependencies, identities, trust roots or services are introduced.
