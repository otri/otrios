<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB designer D1 contract proposal

Date: 2026-11-03. Status: **D1 contract frozen for implementation; acceptance pending**.
Implements declarations for [RFC 0035 D1](rfcs/0035-oib-component-designer.md),
subject to the [execution plan](OIB_DESIGNER_D1.md). Parent reviewed the additive
proposal and both runtime/designer mappings before selecting this attachment.
Runtime implementation and live acceptance remain pending.

The declarations are in `otrios_interfaces::oib::designer`; the existing SDK's
`oib` feature/reexport exposes them without a new SDK dependency or API wrapper.
`Seed.designer` is an optional signed attachment requiring
`otrios-oib-designer-v1`. Absence omits the field and preserves legacy canonical
seed bytes. `Cell`, `Target` and `Allocation` retain their existing encodings.
The next release uses state protocol v27, coordinated across hosts/publisher;
release-control v1 is unchanged. This is not a claim that v27 is deployed.

## Signed metadata and existing implementation

`Document` is the explicit nonvisual OIBDocument owner, with a stable ID/class,
signed class descriptors, definitions, document-owned linked instances and one
presented instance root. Definitions are templates, not painted document siblings.
D1 document-owned objects are instances of definitions; a nonvisual object uses
an OIBObject-rooted definition and has no layout mapping. The minimum presentation
root is a view definition containing an existing layout container. This avoids a
second independent mutable object/layout/value store.

`ClassDescriptor.implementation` names only OIBObject, OIBDocument, O2DView,
O2DPanel, O2DShape, O2DText, O2DButton, O2DToggle or O2DSlider contracts selected
for D1 host admission. A descriptor is data, never class code, an import, or a
grant. Base classes and child rules must agree with that registry; new executable
behavior cannot be declared by metadata. O2DPanel reuses view/surface/scroll
machinery; LabeledSlider is a composed definition. No O2DLabel, text-area/editor, measured
content, typography, image or O3D capability is added.

Class property IDs are stable across the admitted ancestry. Property descriptors
carry existing `layout::ValueType`/`Value`, a signed class fallback, label, display
units, numeric constraints, source policy and inspector section/editor. Only
Number/Boolean/Text/Color are admitted for D1; Series and TimedSeries remain
rejected even though the reused Value enum can decode them. Inspector editors are
ReadOnly, Toggle, NumberSuggestions, Palette and TextSuggestions. Suggestions use
existing `Cell.choices`; numeric bounds/positive step are signed. An arbitrary
free-text or connection form and Presentation::Editor composition are not assumed.
Do not expose an editor the host cannot implement.

`DefinitionNode` separates ownership from ordered visual children. A reference
node is an explicit boundary wrapper around its referenced definition root, not
an alias for that root. Its defaults/placement describe the wrapper; referenced
internal design properties retain their own addresses. The wrapper has no local
owned/visual template children: expansion mounts the referenced root as its
inherited child. Both count toward existing layout/scene work and depth. Each
visual wrapper maps to a distinct existing layout container. Nonvisual references
remain nonvisual. Reference dependencies, ownership and visual containment must
all be acyclic. Every definition has exactly one ownership root; every other node
has one owner. Visual children have at most one visual parent.

`ComponentInstance.owner` is Document or another signed instance, independently of
its optional visual parent. The presented root has no visual parent. Sibling
`order` controls paint order and is unique for that parent. Document ownership and
all connections stay signed in D1; existing admitted root lifecycle is the only
structural mutation. Document, definition and node names are labels, not keys.

`SlotDescriptor` declares a signed host path, accepted classes/subclasses,
cardinality and an assertion about existing layout (Absolute or ExistingFlow).
It does not carry a new flow specification. Check it against the existing signed
layout and signed owned children; a declaration grants no D1 insertion/reparenting.
There is deliberately no slot-content mutation protocol or D2 placement record.

## Stable addresses and exact cell lowering

All ID wrappers are transparent nonzero unsigned u64 identities with distinct
Rust types. Definition node IDs are scoped to their definition; property IDs are
scoped to the admitted class ancestry; slot IDs to the definition. Definition,
instance, class and document IDs have their own namespaces. They are not compact
renderer indices, hashes, display names or traversal ranks.
When lowering into the existing flat Seed, document/definition/initial-instance
raw IDs must additionally be pairwise disjoint and below 0x1_0000, matching its
object-owner namespace. A definition cell's owner is the raw DefinitionId; an
occurrence cell's owner is the allocated root's raw InstanceId. Typed namespaces
do not authorize colliding Cell.owner values.

`PropertyPath { references, node, property }` starts in a known definition. Each
`references` element identifies a reference boundary in the definition reached
so far, then follows its signed definition link. `node` is in the final definition.
An empty reference vector means a local node, not an omitted/guessed root.
`ObjectAddress` prepends the authority root instance ID and omits the property.
It is the identity for selection, focus, semantic targets and captures, including
nonvisual objects. A reference wrapper and its inherited root have different
addresses: `(20, [], 7)` and `(20, [7], 1)` respectively.

`PropertyAddress::Definition` addresses a shared definition/node/property register.
`Occurrence` addresses a root instance plus the complete relative path. For
example `(20, [7,9], 3, 1)` and `(30, [7,9], 3, 1)` are different registers, as are
`(20, [8,9], 3, 1)` and `(20, [9], 3, 1)`. Definition scope is always distinct from
occurrence scope. Compatible rename/reorder fixtures retain every surviving ID,
reference boundary and immutable key. Removed IDs/keys are never reassigned.

`Mappings.definitions` assigns each editable shared definition/node/property an
explicit existing `Cell.id`. `Mappings.roots` assigns each signed factory root a
`RootCellMapping { prototype, cells }`. Every row supplies a full relative path,
an **explicit immutable key in 0..63**, and its initial prototype `Cell.id` alias.
Keys are authored, never assigned from sorting, traversal, descriptor order,
names or renderer slots. Sparse keys are valid. A compatible signed successor
must preserve a surviving path's key and alias. The runtime must validate this
against the admitted predecessor when such an upgrade path exists.

Exact occurrence lowering, using existing O2 constants and checked u64 arithmetic:

1. Find the resident `Allocation` by the full address's root instance ID; select
   its signed factory table using `Allocation.prototype`.
2. Find exactly one row by equality of the **entire** PropertyPath.
3. For `allocation.id == allocation.prototype`, use `row.initial_cell`.
4. Otherwise require `allocation.id >= 0x1_0000`, then compute
   `0x100_0000 + (allocation.id - 0x1_0000) * 64 + u64(row.key)` with checked
   subtraction/multiplication/addition. Overflow rejects before allocation/write.

Example: prototype 20, path `[7,9]/3/1`, key 5, alias 200 maps the initial root to
cell 200. A fresh root 65536 maps it to 16777221; root 65537 maps it to 16777285.
Renaming/reordering that path does not change key 5. The existing implementation
of `instances::cell_id` uses `seed.cells` vector rank instead; integration must
replace that lookup for D1 and must not apply it to these records unchanged.

Validate path uniqueness and key uniqueness per factory, key <64, factory coverage,
nonzero aliases, and global uniqueness of all static definition/initial aliases.
Static aliases remain below 0x100_0000; initial signed instance IDs remain below
0x1_0000. Reject any static/dynamic namespace collision, missing/extra editable
cell or remap. No two distinct property addresses share a mutable register.
The same immutable key may occur in different root factories because only the
chosen factory accompanies a particular monotonic root identity. Never recycle
authority identities; reusable `Allocation.slot` is only a rendering resource.

`ObjectMapping` maps the expanded object to an existing `TemplateNode.id`, or
omits that node for a nonvisual object. A cell's existing `Cell.targets` are its
only render outputs, including scale/offset and semantic-label mappings. Check
them against this signed closure and reject conflicting outputs/type mismatches;
do not copy them into another layout/property store. Runtime-generated inspector
controls are local tooling, not these authored object mappings. Actions/selectors
stay the already signed layout responder/app bindings. Held input pins its full
object identity and active mapping; a renderer slot cannot retarget it.

## Source, defaults, reset and inspector

`DefinitionNode.defaults` lowers to the existing definition cell's signed default,
falling back to the class default only when absent. `AssignmentSource` is Literal,
Forward or AppBinding. Literal instance assignments lower to the existing
`Cell.authored_override`. ComponentReference assignment targets are relative to
the referenced definition; Forward sources are relative to the containing
definition. Top-level ComponentInstance assignments allow Literal/AppBinding
only in D1; there is no implicit document property scope for Forward.

An exposed root property is a normal declared root property/cell with typed target
paths. Its forwarding edges supply those targets' authored-assignment source.
Resolve outside-in, with a live override at each destination taking precedence
there. A forwarded value is evaluated from the current source, not stored as a
copied authored literal. For each destination the precedence remains:

1. Explicit live occurrence override, including an equal-to-inherited assignment.
2. Signed authored literal or current forwarded source.
3. Live shared terminal definition property.
4. Signed terminal component default, then declared class fallback.

Reset causally clears only the live override and reveals the **current** authored
literal/forwarded source. An inner live override can mask outer forwarding without
writing back to it. Reject multiple assignment/forwarding edges to one target,
unresolved paths, incompatible types and forwarding/source cycles. Source chains
and constraints must be checked before coherent publication, not approximated by
the existing one-level `effective()`/`Cell.inherit` logic. Existing owner/name
inheritance validation also assumes a flat component: repeated nested names and
terminal definitions owned by another component need D1 runtime admission work.

An AppBinding reuses `layout::Binding::Local` (the signed selector mapping, not an
arbitrary app field/action ID). Resolve it only through that app's admitted layout
and field schema. Its property is read-only to literal UI edits regardless of the
descriptor's general literal policy; no live assignment, default edit or reset
can turn it into a literal source. The application's already admitted control
behavior can still change app values. Forwarding a bound source stays read-only.
Binding properties need no UI override cell; an attempted override is rejected,
not accepted then ignored. Source connections are not live-editable in D1.

`Selection` and `DesignerMode` deliberately do not derive Serialize/Deserialize.
Selection covers Document, Definition, DefinitionNode and Object (full occurrence,
visual or nonvisual). Mode, expansion, scrolling and selection stay local. Build
inspector sections from the selected object's class ancestry/exposed properties
only. Do not materialize one inspector per resident root in the signed graph.
Origin (class/component/authored/live), source (literal/forwarded/bound), conflict
and active/draft status are derived diagnostics, not another mutable value layer.

## Bounds and rejection

This contract grants no budget increase. Proposed metadata bounds are 16 classes,
64 total declared property descriptors, 16 definitions, and 64 authored objects
(document + definitions + template nodes + signed document instances, counted
conservatively). Reference ancestry <=16 is only an outer semantic ceiling:
the **existing template depth 12, layout instance depth 8, and scene depth 16**
also apply, so no uniform depth-16 nesting support is claimed. Count every expanded
wrapper and inherited occurrence. Existing 512 scene nodes, 128 KiB compiled layout,
control, coordinate, actor/context, message/receipt and 16-targets-per-cell limits
remain. Include all metadata in bounded signed admission; keep the existing 24 KiB
seed-envelope ceiling as the combined Seed + proposed metadata ceiling unless
parent explicitly measures and approves a different bound. Draft plus causal data
still fits 64 KiB, with at most 64 resident editable cells and 16 changes per batch.

Bound all metadata vectors before expansion: each relationship/path/assignment/
forwarding list <=64 entries, class/slot accepted-class lists <=16, all mapping
object entries <=512, total static cell mappings <=64, each root table <=64 rows.
Reference vectors also obey depth limits. Names/labels are nonempty, NUL-free UTF-8,
at most 64 bytes (property display labels at most 96); units at most 16 bytes.
Slot cardinality is ordered, max <=64. Value bounds remain finite numbers in
0..1000, nonempty NUL-free text <=96 bytes, boolean or RGBA bytes. Numeric ranges
are finite ordered subsets; number steps are finite positive and fit the range;
choices (at most 8) and every fallback/assignment must satisfy the descriptor.
No implicit empty-name/default generation or unsupported value coercion is allowed.

Retain six live allocated roots and ten resident descriptors. Cell capacity is
independent: the narrowed companion envelope of six shared +six occurrence cells
per root uses 42 at six live roots but **66 at ten residents**. Preflight the union
of draft/active resident roots, causal/receipt retention and the proposed new root
before allocating an identity or mutating state. Reclaim only when existing rules
permit; otherwise reject capacity. Ten descriptors never promise ten full roots'
cells. No whole-fixture byte/node/control preflight has been run by this contract
task; the actual `apps/oib-designer` fixture is parent/UI integration work.

Unknown fields/variants are codec errors. Well-formed bytes are not admission:
zero IDs, duplicates, unknown class implementations/properties, bad bases, cycles,
missing references, mismatched ownership/mappings, unavailable editors and any
resource overflow reject the candidate without replacing active UI. Conflicting
causal property heads keep the existing exact active graph; admission does not
resolve a conflict by choosing a traversal order. Runtime validation/publication
and its tests belong in LGPL `otrios-core`, not these Apache contracts.

## Canonical encoding and vectors

These are source-level proposal records, not a newly negotiated wire envelope.
For parent review use the repository's locked Serde 1.0.229 and FlexBuffers
25.12.19 `flexbuffers::to_vec` encoding, with existing externally tagged enum names
exactly as declared. IDs encode as u64 values through transparent newtypes; keys
as u8 values; Values use the existing layout serializer. No custom packed struct,
host memory layout, endian transmute, process hash or executable ABI is implied.

Canonical vectors are ordered as follows before serialization. Class/definition/
instance/node/slot/property-descriptor/default arrays sort by their stable ID;
accepted-class lists sort by ClassId. Definitions' exposed rows sort by PropertyId,
their target lists and assignment rows by complete PropertyPath. Root tables sort
by prototype ID and their cells by explicit key (sorting does **not** allocate
keys). Definition mappings sort by (definition,node,property); object mappings by
ObjectAddress. Paths compare lexicographically by reference IDs, then node, then
property. Reference ancestry and visual child order are semantic and never sorted.
Duplicate keys/IDs/paths reject, never deduplicate with last-writer-wins.

Every declared Vec is present even when empty. Only annotated Option fields omit
None, using the existing serde default/skip convention; Some is always explicit.
No other default value is omitted. All booleans/order/constraints are explicit.
Reject nonfinite floats and negative zero before hashing; encode numeric zero as
positive zero. A canonical decoder checks structural/byte limits, decodes,
validates ordering and semantic admission, then requires reencoding to equal the
original bytes (so explicit null for an omitted Option is noncanonical).
Runtime owns that decoder. SHA-256 is over admitted canonical envelope bytes,
using the standard existing implementation; a digest grants neither signature
authority nor access. Parent must freeze complete signed envelope vectors when
choosing the attachment and feature. The small standalone vectors below exercise
record encoding only.

Reference logical records:

```text
Occurrence { instance:20, path:{references:[7,9], node:3, property:1} }
RootCellKey { path:{references:[7,9], node:3, property:1}, key:5, initial_cell:200 }
```

Reference hex bytes are pinned in `crates/otrios-interfaces/contract-checks/designer_codec.rs`.
The standalone check reuses the existing core dashboard dependency graph to obtain
the locked serializers without adding a dependency to interfaces. It performs
contract codec checks only; building core to obtain FlexBuffers is not PoC runtime
acceptance. With the pinned toolchain on PATH and a local CARGO_TARGET_DIR:

```sh
cargo check -p otrios-interfaces --features layout --locked --offline
cargo check -p otrios-sdk --features oib --locked --offline
cargo test -p otrios-interfaces --features layout --locked --offline
python3 crates/otrios-interfaces/contract-checks/check-designer-codec.py
```

## Frozen root model and implementation handoff

Parent reviewed these declarations with the runtime/UI reports. Implement
the explicit key lowering and outside-in source resolver, and qualify rejection,
capture/lifecycle/restart behavior. New `apps/oib-designer` uses a separate companion
app/collection with its own signed volume, brightness and toggle fields. Existing
Live UI app, v26 schema, seeds and data remain unchanged; this proposal authorizes
no migration or seed/schema mutation. Do not activate by simply deserializing this
Document on the old flat runtime. Host/publisher v27 declarations and the required
feature gate are integration-owned. Full fixture budgets and live acceptance
remain unqualified.

The fixture uses four definitions: presentation shell, SettingsPanel,
LabeledSlider and LabeledToggle. The shell is a fixed document instance containing
the nonvisual configuration node. It has no RootCellMapping, allocation record
or live override cells. Its objects/layout/depth/bytes still count. Only the panel
factories appear in Seed.instances/instance_surface and RootCellMapping; their
initial document instances attach visually to the shell. A dynamically created
panel retains the signed prototype's parent attachment while receiving fresh
identity and local derived ordering. The shell is never a seventh allocated root.

`Document.hierarchy_viewport` optionally names an existing signed layout node for
local designer hierarchy chrome. Seed's inspector/preview surfaces remain the
other two tooling panels. Ordinary application documents can omit hierarchy
chrome. Generated hierarchy/inspector rows are local runtime UI, not extra signed
component occurrences or per-resident property records.

These choices resolve the proposal's root-model objection without a second direct
object schema. No claim is made that the complete fixture already fits every
byte/node/control/cell limit; measure before activation.

## Runtime/client implementation boundary

The runtime implementation owns `canvas::oib::document` and these integration
queries. They are ordinary covered Rust APIs, not new wire records:

```text
objects(seed: &Seed, allocations: &[Allocation]) -> Result<Vec<ObjectInfo>>
properties(seed: &Seed, allocations: &[Allocation], selection: &Selection,
           values: &BTreeMap<u64, Option<Value>>) -> Result<Vec<PropertyInfo>>
cell_for_address(seed: &Seed, allocations: &[Allocation],
                 address: &PropertyAddress) -> Result<u64>

ObjectInfo {
  selection: Selection, name: String, class: ClassId,
  depth: u16, layout_node: Option<u16>
}
PropertyInfo {
  address: Option<PropertyAddress>, cell: Option<u64>,
  descriptor: PropertyDescriptor, value: Option<Value>, inherited: Option<Value>,
  origin: String, source: String, overridden: bool
}
```

`objects` returns local hierarchy rows in semantic order, including document,
fixed/nonvisual objects, live occurrence expansion, and definitions. Definition
rows and nonvisual objects do not imply a canvas hit/outline. `properties`
returns only the selected scope's class/component properties and effective
source diagnostics. The caller supplies active or unconflicted draft values;
conflicted alternatives and publication status remain in the existing state
adapter. A missing `cell` is read-only, including app-bound sources; a missing
`value` permits source-only display where app content must be queried separately.
The client never resolves forwarding or computes causal IDs independently.

For normalized numeric toggle fields, `Projection::Nonzero` supplies a Boolean
local selector value. Runtime selector validation/evaluation and OIB sample
compilation must support it. Authoring supplies `StateField::nonzero()` and
`Element::semantic_checked_value(ValueExpr)`; existing static checked helpers
retain their meaning. This avoids a static checked semantic or overwriting the
numeric slider fields to simulate a toggle. The projection is part of v27.

The client implementation owns local hierarchy/inspector generation, selection,
scroll, Edit/Test interception, and occurrence hit routing in `client.rs`,
`client/host_panel.rs`, and a dedicated `client/oib_designer.rs`. It uses these
queries and existing state-adapter property operations. Any required query API
adjustment is coordinated by the parent before both owners diverge. The app
author owns signed fixture/metadata and named-to-compiled-node mappings only.
