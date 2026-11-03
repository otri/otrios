<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB designer D1 contract proposal

Date: 2026-11-03. Status: **additive proposal for parent contract-freeze review**.
Implements declarations for [RFC 0035 D1](rfcs/0035-oib-component-designer.md),
subject to the [execution plan](OIB_DESIGNER_D1.md). No D1 runtime admission,
state protocol, package feature, migration, deployment or acceptance is activated.

The declarations are in `otrios_interfaces::oib::designer`; the existing SDK's
`oib` feature/reexport exposes them without a new SDK dependency or API wrapper.
`Seed`, `Cell`, `Target`, `Allocation`, their serializers, v26 state and the stable
opening release-control v1 envelope are unchanged. Parent owns any future signed
attachment, required package feature and coordinated state-protocol change.

## Signed metadata and existing implementation

`Document` is the explicit nonvisual OIBDocument owner, with a stable ID/class,
signed class descriptors, definitions, document-owned linked instances and one
presented instance root. Definitions are templates, not painted document siblings.
D1 document-owned objects are instances of definitions; a nonvisual object uses
an OIBObject-rooted definition and has no layout mapping. The minimum presentation
root is a view definition containing an existing layout container. This avoids a
second independent mutable object/layout/value store.

`ClassDescriptor.implementation` names only OIBObject, OIBDocument, O2DView,
O2DShape, O2DText, O2DButton, O2DToggle or O2DSlider contracts already implemented
and admitted by the host. A descriptor is data, never class code, an import, or a
grant. Base classes and child rules must agree with that registry; new executable
behavior cannot be declared by metadata. Panel and LabeledSlider are composed
definitions using those implementations. No O2DLabel, text-area/editor, measured
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
independent: the proposed companion envelope of 20 shared +6 occurrence cells
per root uses 56 at six live roots but **80 at ten residents**. Preflight the union
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

## Integration decisions still required

Parent must freeze/adjust these declarations with the runtime/UI reports, implement
the explicit key lowering and outside-in source resolver, and qualify rejection,
capture/lifecycle/restart behavior. New `apps/oib-designer` uses a separate companion
app/collection with its own signed volume, brightness and toggle fields. Existing
Live UI app, v26 schema, seeds and data remain unchanged; this proposal authorizes
no migration or seed/schema mutation. Do not activate by simply deserializing this
Document on the old flat runtime. Required feature/envelope/version selection,
both host codecs, full fixture budgets and live acceptance remain parent-owned.

One deliberate modeling tension needs parent review: this minimal proposal makes
all document objects definition-backed, including the presented view container.
If the planned three definitions are exclusively SettingsPanel, LabeledSlider and
Toggle, a separate plain presentation shell requires a fourth small definition,
or a revised direct-object/root record before freeze. It is not hidden in the
three-definition budget. A fixed shell without editable properties need not add
causal cells, but its objects/layout/depth/bytes still count. Likewise, parent must
decide how fixed document shell objects coexist with O2's allocated root tables;
this contract does not grant a seventh allocated root. No claim is made that the
proposed fixture already fits these records and all existing limits.
