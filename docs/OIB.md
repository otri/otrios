<!-- SPDX-License-Identifier: Apache-2.0 -->
# Live OIB UI

OIB O1 implements the property-editing slice of
[RFC 0029](rfcs/0029-live-oib-ui.md). Its original qualification is recorded on
[browser and Frame](experiments/live-oib-2026-11-02.md).
[O2](rfcs/0030-live-oib-instances.md) adds bounded live creation, duplication and
removal through shared typed operations. Its
[live qualification](experiments/live-oib-instances-2026-11-03.md) records Frame
input, live API edits and the deployed browser Wasm client observing the same data.
Executable main-class and outlet APIs remain later increments.

[O3 component geometry](rfcs/0031-live-oib-component-geometry.md) adds declared
live layout properties: label and track placement, widths, heading visibility and
row pitch. These use the same CRDT property batch as labels/colors. The component
definition still admits the available targets; arbitrary tree editing and panel
constraints remain outside this surface. The graphical inspector currently exposes
its original property rows; geometry can be edited through the typed host API.

Add the installed **Live UI** app to a Space using its Admin controls, then open
**Test Playground → Live UI** in that same Space (or launch its Space tile).
The screen starts with `volume` and `brightness` instances of `LabeledSlider`.
Use **Add LabeledSlider** to create another instance. Tap an instance heading to
inspect it, then use **Duplicate** or **Remove**. The preview and inspector scroll
independently; pan the preview background or use a wheel over it to reach lower
instances. The inspector contains the component and your locally selected instance.

Added sliders use the declared Volume binding; duplicates retain their source's
Volume or Brightness binding and copy its active overrides. Binding names are
shown in the headings. Creating new app-content fields or reconnecting bindings
is outside O2. The current delivery admits six live instances, with bounded
resident draft/active records and reusable renderer slots. There is no ten-object
lifetime allocation budget: deleted objects' properties, presence and descriptors
are reclaimed once neither draft nor active UI uses them. Fresh IDs come from a durable monotonic
counter; delayed operations for deleted IDs are acknowledged and discarded.
The status shows available instance slots. **Remove pending** resolves an
unactivated new instance, such as a concurrent duplicate-name conflict.

**Restore sliders** brings back the authored Volume/Brightness pair and bindings,
while keeping the live component design and overrides on surviving objects.
Deleted objects are recreated with fresh IDs and inherited properties; extras
are removed and reclaimed. **Reset UI defaults** also restores signed
component defaults and clears instance overrides; it preserves current slider
values. These operations preserve surviving authored objects and reclaim unused slots.
See [restoration](rfcs/0032-oib-restoration.md) for causality and compatibility.
The signed component defaults now use the approved compact row: 128 px label,
288 px track, 76 px percentage column and 80 px row pitch. Added sliders inherit
the shared component geometry. Restore keeps later design edits; Reset UI defaults
returns to this compact design. The signed default update retains existing
property registers, allocations and app values until an owner explicitly resets.

- **Change** cycles through suggestions authored by the app for label, accent,
  width or spacing. Component edits affect both instances unless overridden.
- Changing an instance property sets its override. **Reset** clears the override
  causally and restores inheritance. The inspector identifies inherited values
  and explicit overrides.
- **Default** explicitly writes the signed seed default, observing all retained
  alternatives. It can resolve a same-property conflict. The typed client API can
  also resolve to another chosen value.

Space Admins edit UI; Members and admitted visitors receive the active result.
The authority enforces this independently of visible controls. Edits affect only
the selected Space's Live UI collection. The demo uses its own volume/brightness
content fields; existing app schemas and values are unchanged. App content values
remain independent of UI properties. Scroll position, gestures, inspector interaction and
focus remain local.

Every accepted change is durably journaled before publication. Disjoint changes
merge; different concurrent assignments to one property stay in the draft. A
conflicted or invalid draft retains the authority's previous active graph for all
viewers. Editors receive status and affected property identities. Layout activation
waits for held gestures and unconfirmed app edits; it retains the current mount,
state, responder identities and renderer resources.

## Local rendering and input

The inspector scrolls through a local viewport offset and clipping. Its geometry
and glyph buffers stay resident while scrolling; frames update placement and clip
ranges. Visible semantic hit rectangles are clipped independently. Scrolling never
writes shared OIB data. The runtime still traverses scene placements and updates
hit regions on the CPU; it does not retessellate the panel or upload its mesh for
each scroll sample.

The preview and inspector use independent, ordered retained layers. Changing a
slider percentage does not rebuild the inspector. OIB display strings and
resource diagnostics are cached by the admitted OIB root and pending UI intent;
ordinary app writes do not produce a misleading “Saving UI change” indicator.
The native owner yields between bounded input turns so continuous dragging cannot
starve presentation or state sync. Property intents retain client type/identity
checks; authority validation, durable publication and received-state validation
remain authoritative. A property delta does not repeat package admission.

Very large scenes that exceed the existing 65,536-vertex retained-view range use
the clipped fixed-placement renderer; this bound does not affect the Live UI
panel. The [Frame input performance record](experiments/frame-oib-input-2026-11-02.md)
contains measurements and remaining limits.

## Contracts and storage

`Presentation::Oib(Seed)` in Canvas UI v8 contains `main_oib`, `main_class`, named
component/instance identities, the fixed property table and admitted mappings to
layout nodes. O1 uses `otrios.declarative-v1`, the existing declarative event
interpreter, without adding guest imports or an execution backend. The immutable
bundle requires `otrios-canvas-v8`, `otrios-oib-v1` and, for an instance surface,
`otrios-oib-instances-v1`. Old hosts reject these
requirements; they cannot silently interpret the OIB as legacy UI.

The public contracts live in `otrios_interfaces::oib`, also reexported as
`otrios_sdk::oib` with the SDK's `oib` feature. Application authoring uses
`AppUi::with_oib`, `NamedOibTarget` and the same shared layout builder as other
apps. App-defined inspector choices are data; the runtime contains no slider-
specific labels, colors or property names.

`Client::instantiate_oib(component_id, name)`, `duplicate_oib(instance_id, name)`
and `remove_oib(instance_id)` use the same operations as the inspector.
`edit_oib_instance_property(instance_id, "label", Some(Value::Text(...)))`
addresses a declared named property; `None` clears its override. The corresponding
StateAdapter methods use the same admitted property table. Wasm host exports
`oib_instantiate`, `oib_duplicate`, `oib_remove`, `oib_property` and `oib_inspect`
expose this host API; they do not add a guest Wasm import or execution backend.
Wait for confirmation after creation, then find the allocated stable ID among
`resource_usage().oib.instances`, filtering for active instances. Local inspection
uses `Client::inspect_oib` and never emits a shared operation.

`Client::edit_oib(&[(cell_id, Some(value))])` and
`StateAdapter::edit_oib` use the same typed operation as the inspector. `None`
causally clears an instance override. Only one issued OIB batch is retained per
writer; wait for confirmation before submitting another. A batch has at most
16 distinct cells. Observed leaf snapshots are captured before issuing it.

Property writes use tag 37 in state protocol **v25** (introduced in v21): u64 writer sequence followed
by a bounded blob containing the canonical FlexBuffers vector of `Change` values.
Tag 38 carries one bounded canonical InstanceOperation under the same writer
sequence/receipt contract. Allocation, draft/active changes and receipt are
journaled before publication. A rejected draft keeps the exact active root.
Each property change contains a stable u64 cell ID, optional typed value and the observed
canonical Merkle leaf. The authority checks observed context against its retained
context, then assigns against that observation and joins concurrent values. It
does not turn a stale observation into a write against the newest authority head.

The collection's existing Canvas root adds branch 4:

| Path | Contents |
| --- | --- |
| `[4, 0]` | Immutable owning UI reference, signed seed descriptor and SHA-256 of its canonical admitted UI |
| `[4, 1, <13 base-32 digits of CellId>]` | Draft causal property register |
| `[4, 2, <13 base-32 digits of CellId>]` | Active resolved property register |
| `[4, 3]` | Active generation/root, source draft hash, conflict IDs and bounded error |
| `[4, 4]` | Resident authority descriptors with reusable renderer slots |
| `[4, 5, <13 base-32 digits of InstanceId>]` | Draft causal presence |
| `[4, 6, <13 base-32 digits of InstanceId>]` | Active resolved presence |
| `[4, 7]` | Durable next object ID; never decreases or reuses a deleted ID |

Cell paths have depth 15 and fanout at most 32. The current implementation stores
the fixed descriptor table once in metadata instead of at every cell. Clients
receive changed Merkle cells; compiled layout bytes remain derived from their
verified package. The Canvas checkpoint, append journal and writer receipts cover
UI changes and ordinary app edits in one publication root.

Admin coverage includes the draft. Other participants receive metadata, active
cells and bounded status, excluding conflicting draft values. A role change
invalidates the admitted grant and drops its private mount. The stable opening
release-control v1 envelope, shared WebSocket and release discovery path remain
independent of the new state codec.

O1 admits at most 16 definitions, 64 authored objects, 64 editable cells, 8
inspector suggestions per cell and 24 KiB of encoded seed descriptors. The draft
branch is bounded at 64 KiB, including causal metadata. Text properties are at
most 96 UTF-8 bytes; numeric property values must be finite in 0..1000. Declared
boolean properties control visibility. Compiled
geometry, layout bytes, expanded scene/control counts and both supported logical
orientations must also qualify before activation. Existing Merkle actor/context,
message, receipt and journal limits continue to apply.

## Current boundaries

Component factories and bindings are fixed by the signed seed. O2 creates,
duplicates and removes instances under the main OIB in authority allocation
order. Reparenting, arbitrary template edits, new code connections, guest UI
mutation imports and general seed rebasing are not implemented. The signed O2
surface permits one exact O1 upgrade, preserving its property contracts, values,
conflicts and app data. O2 bundles require `otrios-oib-instances-v1`; derived
layout v5 reserves sparse responder identities without increasing control-count
limits. Earlier layouts retain their event-ID bound. An incompatible seed reports unavailable retained
UI rather than silently discarding its overrides. Resetting one property is an
ordinary causal edit; code rollback never restores old app data.

Signed app presentation updates may retain the exact canonical OIB seed. Those
updates change the admitted UI hash while preserving live descriptors,
presence, property registers and app values. They do not
reinitialize instances. Changed seed definitions still require an explicit
compatible migration. The component-geometry increment explicitly appends
component cells from exact signed predecessors while preserving old contracts,
allocations and values. A signed presentation update is distinct from a live CRDT
edit: panel layout constraints are not yet editable OIB properties.

Geometry-enabled bundles require `otrios-oib-layout-v1`. Component targets are
remapped to each instance without adding per-instance geometry registers. The
`row pitch` cell controls both instance placement and preview scrolling extent.
Painted content must fit its row; invalid geometry keeps the previous active
design. Layout activation waits for held gestures and preserves the mounted app.
`Client::edit_oib` accepts one atomic batch; browser host `oib_edit` accepts the
same `(cell ID, optional typed Value)` pairs as bounded JSON. The
[layout RFC](rfcs/0031-live-oib-component-geometry.md) lists the compact-row cells.

The inspector currently offers app-authored choices. The typed client API accepts
other values within the admitted property type and limits. A general free-text
property editor remains separate UI work.

**Portable data export/replacement for Live UI remains unavailable in O2.** Its
current codec does not yet include the OIB
namespace, so the authority refuses the entire transfer instead of omitting UI
state. Other apps retain their existing portable transfers. Signed app sharing
continues to contain immutable code/seeds only.

Runtime implementation and tests remain under LGPL-3.0-only in
`crates/otrios-core`. Contracts, SDK declarations, app authoring and tools retain
their Apache-2.0 boundaries. No dependencies, identities, trust roots or services
are added by O1.
