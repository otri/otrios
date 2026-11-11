<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer: technical design

Date: 2026-11-11. Status: **proposed implementation target**. This document is a
contract design, not evidence of implemented Auto Layout, named styles or a new
editor release. Read [the package entry point](README.md),
[UX journeys](UX_JOURNEYS.md), [visual reference](../../DESIGN.md) and
[RFC 0037](../rfcs/0037-oib-design-system.md) together. The HTML prototype validates
selected design-model transitions; it does not substitute for native/Web runtime
admission, input, signing or performance acceptance.

## T01. Existing foundation and explicit gaps

| Concern | Existing implementation | New design work |
| --- | --- | --- |
| Canonical source | OIB v2 graph, semantic property addresses, definitions/presets, instances and typed source connections | Auto Layout records, richer component contracts and named-style references |
| Editing | Generation-checked structural EditBatch; independent causal literal registers | Coherent local working copy and explicit Apply across the new field types |
| Appearance | Definition-node defaults; compiled theme constants; limited live colors | Typed named styles, variable modes, source-aware full appearance inspector |
| Runtime | Shared Rust scene/layout/input, Wasmi component ABI 1; library-owned Slider | New generic layout compiler and incremental dependencies, additional admitted controls |
| Packaging | Verified exact signed library closure; compatible L1/L2 adoption and rollback | Contract fingerprints/capability admission for layout/styles/new exports |
| Editor shell | Frame Focus and resizable Desktop Studio, retained hierarchy/inspector scrolling | Consistent full lifecycle, impact review, catalog qualification and draft handling |

Sources of truth are `crates/otrios-interfaces/src/oib/{designer,graph,library}.rs`,
`crates/otrios-core/src/canvas/oib/`, `canvas/client/oib_designer*`,
`docs/OIB_D2_COMPLETION.md` and `docs/OIB_LIBRARY_L2.md`. `graph::Visual` currently
contains frame expressions and ResizeRule; Source has Literal, Forward and
AppBinding. No existing StyleId, variable mode or generic Figma-compatible
Auto Layout contract is assumed. Earlier pins/struts remain a historical/internal
capability; the new authoring model exposes no constraint solver or pin editor.

## T02. Ownership model and canonical records

The document contains definitions, presets, instances, local design systems,
explicit dependency locks, connections and a presented root. An editable component
is a **definition**, its palette defaults are a **preset**, and a placement is an
**instance**. Do not use one mutable object to represent all three.

Every reference has an owner namespace and stable semantic ID. Human-readable
names can change. Source IDs never depend on renderer slots, row order or the
current signed package hash. Exact content revisions still identify immutable
bodies. The same semantic ID cannot silently change type or meaning.

Proposed conceptual records, before a concrete codec is selected:

```text
Definition {
  id, name, root_node, nodes, exposed_properties, exposed_actions,
  slots, style_contract, lifecycle: Active | Retired
}
Node {
  id, name, kind: Primitive | Container | Reference(definition_ref),
  ownership_parent, ordered_visual_children, layout_item, layout_container?,
  property_sources, style_use?, semantics?, action_routes?
}
Instance {
  id, definition_ref, preset_ref?, owner, visual_parent, slot?,
  local_property_overrides, local_style_overrides
}
DefinitionRef = Local(definition_id) | Imported(library_id, export_id, exact_revision)
```

These shapes explain semantics; they are not current accepted package bytes.
Continue using the single canonical DocumentSource rather than introduce parallel
mutable ownership, placement, style or connection stores that can diverge.
Derived dependency indexes and compiled scene data are disposable caches.

A new blank visual definition starts with one explicit empty Container root,
Vertical flow, design width Fixed 320, height Hug with minimum 48, padding 16,
gap 8 and no visual background. The empty drop-zone and selection outline are
editor chrome, never authored children. A blank definition can Apply as a valid
nonexported draft; publishing an empty control requires an explicit choice and
cannot pretend it has interactive behavior. Insertion always allocates fresh
node/instance IDs after complete candidate admission. A rejected batch consumes
no IDs. Duplication recursively remaps owned IDs while retaining external refs.

Reference boundaries remain visible in Layers. Editing a referenced child means
an instance override or deliberate navigation to its owning definition; it never
silently mutates the imported body. Make local copy/fork creates new definition
identities and explicit mappings, without claiming ownership of an imported ID.

## T03. Layout authoring contract

### T03.1 Core fields

Container fields are flow Horizontal/Vertical, padding per edge, gap, main-axis
packing Start/Center/End/SpaceBetween, cross-axis alignment Start/Center/End/Stretch,
clip flag and an optional scroll axis. Children have per-axis Fixed/Hug/Fill,
minimum/maximum length and an explicit `InFlow` or `Free` placement. A UI nine-point
alignment control maps to physical horizontal/vertical group alignment; its saved
meaning is normalized to main/cross axes when flow changes. Display both axes in
the inspector so rotating flow does not hide the resulting behavior.

Fixed carries a finite nonnegative logical length. Hug derives intrinsic size.
Fill consumes an available allocation; it is not an unbounded request. An absent
minimum is zero, absent maximum is the admitted finite coordinate bound. Reject
negative padding, invalid bounds and min > max. Core gap is nonnegative; overlapping
items require explicit Free placement rather than negative-gap surprises. Text
uses its admitted measured intrinsic metrics and wraps only when a finite width
is supplied. An empty container's Hug size is padding plus its minimum.

No layout property implies semantic interactivity. A 16-unit icon can be inside a
48-unit control target. Target enlargement must remain within its owning control
and cannot overlap unrelated targets. Hit testing uses the final clipped geometry.

### T03.2 Deterministic core algorithm

The compiler builds a dimension-dependency graph before allocating draw resources.
Measure intrinsic content bottom-up, allocate available container lengths top-down,
then place children. Work with the existing renderer's canonical layout units and
checked finite arithmetic; specify rounding fixtures before serializing a new ABI.
The implementation may use more passes internally, but must not iterate until a
visual guess happens to stabilize.

For a definite main-axis inner length A, subtract gaps and the clamped measured
lengths of non-Fill children. Allocate remaining space equally among Fill children.
Clamp each to its min/max, freeze a clamped child and redistribute residual space
among unfrozen Fill children until no child freezes or none remain. This bounded
water-fill algorithm takes at most the number of Fill children iterations. If
minimum lengths already exceed A, retain minima and report overflow; do not make
negative lengths or shrink text below its admitted measure. Spare space after all
maxima is handled by packing. SpaceBetween distributes nonnegative spare space
between children in addition to minimum gap; for zero/one child use Start.

Cross-axis Fill/Stretch uses definite inner length, clamped by child bounds.
Fixed/Hug children retain their size and use cross alignment. If a child's minimum
exceeds the container it overflows predictably. Stretch does not override an
explicit Fixed dimension: the inspector offers Fill when the author wants stretch.
Hug container main length is padding + child measured lengths + gaps. Hug cross
length is padding + maximum child measured cross length. Bounds clamp the container
result; clipping/scrolling controls presentation, not hidden geometry mutation.

Fill in a Hug ancestor's same indefinite axis is rejected with a dependency path:
“Root height hugs children; Slider height fills Root. Use Hug/Fixed on Slider or
Fixed/Fill on Root.” Do not silently reinterpret Fill as Hug. Fill in the opposite
definite axis is legal. Empty Fill containers require a definite allocation from
an ancestor/design viewport. The presented root has an explicit design viewport,
so root Fill is defined there; a component root's design frame is an authoring
preview size, not a forced size on every instance.

Text wrapping can create a width→height dependency. A vertical Hug stack with a
Fixed/Fill definite width may measure multiline text height normally. A parent
whose width Hug depends on wrapped child height and whose height depends back on
that width produces a diagnosed cycle; reject the cycle rather than use repeated
layout heuristics. Font/style changes can invalidate both dimensions.

### T03.3 Position, scrolling, clipping and transforms

Free children are positioned in their containing padding-box coordinate system
with explicit x/y and size rules. They do not contribute to Hug size or main-axis
space distribution. Warn when all children are Free in a Hug container. Layers
order is the paint order; explicit Move earlier/later remains available without
dragging. Reparenting preserves semantic identity but recomputes inherited style,
mode and size; show the resulting preview before Apply.

A scrolling container needs a definite viewport length in its scrolling axis;
its content can Hug beyond that extent. Same-axis unbounded nested scrolling is
not accepted in the initial core; offer a definite viewport or one scrolling
owner. Clipping intersects visual and input geometry. Decorative selection, focus
and layout overlays pass through input; dialogs use an explicit modal barrier.
Do not turn off a modal barrier merely because its visual background is transparent.

Rotation/flip controls are only enabled after transformed paint and inverse hit
geometry are admitted together. Rotation is not a workaround for flow direction.
Free positioning is core; general constraints, alignment-to-arbitrary-sibling,
baseline constraint graphs and absolute percentage anchoring are deferred.

### T03.4 Wrap and grid extension gates

Core Horizontal/Vertical must be complete before enabling Wrap/Grid. Their entries
can be shown as unavailable with a concrete explanation, not as controls that save
unsupported values. Wrap requires a definite line width, deterministic source-order
line breaking, row/column gaps and line alignment. A Fill child in Wrap must have
an explicit basis/minimum for line admission; it cannot consume an infinite row.
Grid initially uses a fixed positive column count and equal tracks, row-major
ordering, explicit gaps and Hug row heights. Spans, masonry and content-dependent
track solvers are deferred. Both extensions need golden layout/hit fixtures,
resource measurements and a new capability gate; they cannot piggyback on HTML
CSS behavior as proof that the shared Rust engine implements them.

## T04. Component composition, API and behavior

A LabelledSlider is a vertical Container with Text and a Slider reference, not a
new host-coded widget class. The definition exposes `label: Text`, `value: Number`,
`minimum/maximum/step: Number`, `enabled: Boolean` and an optional style reference
of the declared contract. Root `label` forwards to Text.content; root numeric
properties forward to the imported Slider's admitted endpoints. Initial Number
range restrictions must remain within the current admitted numeric domain until
qualified extensions are introduced.

```text
LabelledSlider definition 100
  root 101: Vertical; width Fill; height Hug; gap 8
  label 102: Text; width Fill; height Hug; content ← root.label
  slider 103: Ref(core2d.Slider); width Fill; height Fixed 48
              value ← root.value; enabled ← root.enabled
  exposed changed(Number) → Slider's declared changed outlet
```

The root's Fill width resolves against its instance allocation; the blank
component design viewport supplies its preview allocation. This example is
conceptual: the existing Slider numericSet ABI and admitted targets remain the
implementation boundary until these exposed routes are concretely validated.

Exposing a property means creating a stable typed endpoint and a typed forwarding
edge. It does not copy the current child value. Type mismatch, unresolved endpoint,
multiple source writers and source cycles reject the candidate. Rename preserves
endpoint identity. Deleting/changing an exposed endpoint is a contract change;
show affected references and require repair before Apply/publication.

Actions route declared events to declared destinations under existing grants.
A binding UI only lists admitted app fields/actions and compatible types. It
cannot invent an arbitrary application field or execute source text. Bound values
are read-only to literal override editing; changing the value is a Test-mode
interaction routed through the declared behavior. Removing a binding is an
explicit structural edit if authorized, not an accidental color/property Reset.

Slots declare stable identity, accepted component class/contracts, ordered
cardinality and defaults. Inserting a Slider into the content slot adds owned
content and a reference. It does not edit the referenced Slider definition.
Replacing a slot's default does not overwrite explicit instance slot contents.
A missing required slot prevents Apply. Slot cycles and containing-definition
self-reference are rejected, with the reference path shown.

Component runtime code remains signed, bounded Wasmi component ABI 1. Designers
compose existing behavior and expose its typed routes; arbitrary graph-authored
programming, native plugins and general Wasm authoring inside the editor are not
required for this lifecycle. Date/time, password, file, image, chart and navigation
controls need their own declared contracts; a catalog thumbnail grants no capability.

## T05. Styles, modes and property source precedence

RFC 0037 is normative for proposed style resolution. The editor's main distinction
is Definition vs Instance, then style vs field value source. Definition changes
update unoverridden linked instances; instance edits remain local. A named style
can be shared by unrelated components, so its editor must show known affected
uses and owner before Apply. There is no required global Accent.

Display `background`, `text`, `stroke` and control-specific fields such as `track`,
`thumb`, `placeholder` and `selection`. A field shows Literal, Variable, Style,
Forwarded, App-bound or Override as applicable. Source rows are diagnostics, not
another value store. A variable is referenced by owner+ID, with independent
Light/Dark values. Labels/names are never runtime lookup keys.

Normalize RFC 0037 style default/state recipes first, then document and instance
style overrides. Exposed property forwarding takes place through the existing
canonical graph's typed assignment resolution. For a terminal appearance field,
a proven explicit property assignment/occurrence override outranks the style's
contribution; the compiler must admit only one unambiguous resulting source chain.
An app-bound appearance property cannot be overridden by a style. The inspector
shows the actual winning chain, never two separate “effective color” answers.
Final concrete mapping between style field IDs and PropertyAddress is an explicit
codec/admission gate, not an assumed current capability.

Reset clears only the chosen override scope and reveals today's inherited source.
If an inner occurrence masks an outer forwarded value, show that relationship.
Hover/pressed/disabled recipes are color-only in the first delivery. Focus is a
separate overlay and enabled/disabled semantics remain input-owned. State preview
and Light/Dark preview are local editor context, not shared app writes. Literal
colors do not adapt; no automatic inversion or foreground guessing occurs.

A contrast diagnostic references actual foreground/backdrop usage and mode/state.
It can be Pass/Warning/Unknown, never fabricate a ratio over unresolved imagery.
Base catalog acceptance follows DESIGN.md's text/non-text targets; custom authors
can retain warnings deliberately. This is not blanket accessibility certification.

## T06. Working copy, Apply and conflict behavior

The new UX uses one per-editor local working copy based on `(document epoch,
confirmed structural generation, captured property causal contexts, exact imports)`.
It is distinct from the existing shared causal draft/active branches. Do not reuse
“draft” to claim another person's active source is a private scratch space.

Local edits update the working copy and preview, forming a bounded command log.
They do not mutate the shared document until Apply. Apply submits one bounded
candidate batch with its captured preconditions. The authority validates scope,
authorization, types, topology, dependencies, resource ceilings and compilation,
then publishes one complete revision and a durable idempotent receipt. No partial
“layout applied, style failed” publication is allowed. If existing independent
property and structural operations cannot provide this atomic grouping, first
extend the transaction contract within the bounded OIB document; do not simulate
atomicity by issuing several visible writes.

| Editor state | Allowed operations | Visible result |
| --- | --- | --- |
| Clean/confirmed | Edit, Test, publish confirmed source | Current generation |
| Dirty/valid | Continue, preview/Test, Apply, discard | Unsaved local changes |
| Dirty/invalid | Correct, inspect issue, undo, discard | Apply disabled; last valid preview marked stale |
| Applying | Continue viewing; do not issue another Apply | Pending receipt, old confirmed active |
| Rejected stale | Inspect remote diff, explicitly refresh/reapply compatible choices | Local copy retained; no auto overwrite |
| Disconnected | Local edits/Test of available resources | Apply/publish unavailable; no false Saved label |
| Result uncertain | Reconcile same operation ID/receipt before retry | Do not allocate a second object |
| Permission revoked | Inspect/copy permitted local choices, discard | Write commands blocked; no silent Apply |

On concurrent update, retain local edits and show per-field before/local/current
and structural paths. Automatic rebase is limited to nonconflicting metadata
only if later explicitly approved; initial delivery requires deliberate refresh
and reapplication. Structural and property conflicts retain existing causal
semantics. Apply cannot substitute a new epoch for a queued old-epoch edit.

Cancel/Discard removes local edits; it does not revert other clients' confirmed
changes. Leaving the editor with dirty work offers Apply, Discard or Stay. Browser
reload/crash recovery of local work is best effort only and not a promised durable
workspace; explain this before leaving. A bounded app-local recovery draft may be
added later, with no offline edit preservation requirement inferred from this RFC.

### Undo/redo

Before Apply, undo reverses local commands and redo reapplies them without network
traffic. Coalesce a numeric/color drag into one logical command. After Apply,
“Undo applied change” is a new conditional edit against the current generation and
observed values. It succeeds only when its affected sources still equal the edit's
postcondition. If another author touched them, open a comparison instead of
rewinding their work. Recreating a deleted object uses fresh IDs unless the
existing reclamation rules explicitly permit resurrection; never reuse retired
identities that a delayed message could target. No global time-travel transaction
or entire-document snapshot restore is used as ordinary Undo.

## T07. Dependency-safe removal, replacement and retirement

Build a reverse dependency index from the canonical candidate and every admitted
local reference: definition refs, presets, instances, slot defaults, exposed
forwarding, action routes, style/variable refs and exact imported exports. Keep
index revision equal to the source being reviewed; stale counts block destructive
Apply and force refresh. The index is a derived cache, not authority.

| Operation | Permitted effect | Required impact handling |
| --- | --- | --- |
| Delete instance | Remove selected occurrence and its owned content | List disconnected bindings/owned subtree; leave definition and app data |
| Delete child | Remove owned node/reference | Repair exposed properties, required slots and routes in same candidate |
| Delete unused definition | Remove local definition/presets with no references | Full candidate validation; list presets removed |
| Delete in-use definition | Block by default | Offer inspect uses, replace references, or retire |
| Retire definition/export | Hide from new insertion, preserve current uses | Existing refs/rendering remain valid; unretire is explicit |
| Replace local references | Repoint chosen references with explicit mapping | Validate property/action/slot/style compatibility and overridden values |
| Delete variable/style in use | Block unless same batch repairs uses | No silent literal flattening or dropping state variants |
| Remove library import | Only after no reachable candidate refs remain | Other libraries/apps and old signed closures unchanged |

Dependency counts are scoped: “3 uses in this document” is defensible. Do not claim
“unused everywhere” without a proven complete registry. Local retirement cannot
remove bytes from previously signed immutable closures. Old apps continue using
their exact signed pins. A new library revision removing a public export is a
breaking contract and cannot pass the compatible-update path merely because the
current editor sees no local use. Publish a deprecation/retirement update first;
actual removal requires new admission and dependent-app repair.

Replacement is an explicit dry-run plan: source→destination, affected paths,
preserved values, unmapped overrides/bindings, slot cardinality and semantic role
changes. Require user decisions for unmapped fields; type equality alone does not
prove meaning equality. Apply all local replacements and deletion in one document
batch or do nothing. Multi-app replacements are separate reviewed publications,
not an invented cross-app atomic transaction. A cancelled/failed replacement keeps
the source active. Auto-cascade deletion is not offered in the first delivery.

## T08. Edit mode, Test mode and shared runtime semantics

Edit mode routes pointer input to selection and editor commands; a Slider's drag
selects/moves only through an explicit handle and never changes business values.
Test mode sends input into a sandboxed preview state adapter initialized from
sample values. It cannot emit real app effects. Its sample values reset on explicit
Reset test data or reopening the test session, not on every repaint. The preview
must use the same admitted layout/input/behavior compiler as the real application;
HTML-native controls in a prototype are acknowledged approximations.

The local preview's semantic identity uses definition+occurrence path. Recompiling
an unrelated property preserves selection, expansion, scroll anchors and focus
when that object survives. If it is deleted, select the nearest surviving owner
and announce the change. Style/mode edits preserve active input capture. Structural
changes that remove a capturing control cancel that local preview gesture before
swapping, and library updates use the established confirmed-edit/gesture barrier.
Published applications must not be made busy by invisible Designer test state.

## T09. Frame and desktop workspace

Target native Frame logical landscape **1280×800** and desktop **1920×1080** first;
verify actual calibrated output/rotation through existing Frame hosting. Historical
800×1280 pixel captures are not proof that the new editor must support portrait.
Phone portrait is explicitly unsupported; show a readable “Use a landscape Frame
or desktop window” notice rather than compress the inspector into unusable forms.
Proposed support floor is 1024×640 landscape, to be qualified; 1280×800 is the
required Frame acceptance viewport, not an assumed minimum scene resource limit.

Desktop uses persistent Library/Layers on the left, canvas in the center,
Properties on the right, and a compact status/Apply region. Frame uses canvas plus
one task panel at a time: Library/Layers or Properties. Both panels can be shown
only if canvas remains useful. Panel switches retain selection, expansion,
scroll and typed drafts. No offscreen panel should materialize its entire scene.

Preferred widths: desktop left 240, right 320, canvas remainder; Frame task panel
320–360, canvas remainder. Toolbar ~56 and footer ~40 leave the working height.
These are design targets, not admission overrides. A 48-unit touch target applies
to Frame actions, including disclosure and close controls. Compact desktop targets
need separate input-profile qualification. The software keyboard must avoid the
active field; shrinking canvas is allowed, shrinking the text is not. Scroll the
inspector to the focused field and restore its anchor when the keyboard closes.

Selection overlays are local chrome and input pass-through. Canvas zoom/pan is
local, preserving authored logical sizes. Fit to component is available; numeric
position/size controls and Layers Move earlier/later provide alternatives to
precision dragging. Multi-selection, freeform vector editing and docking-window
systems are not prerequisites for the complete single-component lifecycle.

## T10. Compilation, invalidation and scene budgets

Compile from one pinned confirmed or working-copy revision. The same core compiler
produces layout, scene, semantics, control routing, dependency maps and diagnostics.
Hosts do not each implement their own layout resolver. Offscreen content may need
measurement for Hug/scroll extent but does not require GL resources until visible.
Virtualized Layers/Library/Properties use retained windows, with existing scene,
control, coordinate and byte ceilings unchanged.

| Edit | Minimum invalidation when proven | Conservative fallback |
| --- | --- | --- |
| Name or source label | Inspector/Layers metadata | No layout solely for rename |
| Resolved color | Affected retained paint slots | Full compile for untracked/composed dependency |
| Mode/state color | Affected inherited color uses | Stop at explicit mode boundary; fallback when proof fails |
| Radius or stroke width | Affected shape geometry | Layout if bounds/hit extents change |
| Content/font/padding/gap/size | Dependent measurement/layout and resulting paint | Full candidate compile |
| Reference/slot/structure | Reexpand affected graph and validate all identities | Full candidate compile |

Extend the existing paint Basis with exact style/variable/layout dependency
revisions. A color-valued expression is not automatically a proven paint-only
change. Compare resolved outputs before marking dirty. Coalesce multiple field
edits into the strongest needed invalidation once per frame. Incremental pipeline
correctness comes before parallelism; independent immutable measurement can be
parallelized only after actual profiles justify it. No 100K elements/sec claim is
made by this design.

Existing qualified scene ceiling is 512 nodes/128 controls, with other OIB graph,
source, depth and byte ceilings separately enforced. New editor chrome, open
keyboard, modal and preview must fit jointly, not each in isolation. RFC 0037's
proposed style caps are additional upper bounds, never permission to raise the
stricter existing graph caps. A resource rejection names the limiting dimension
and offers a smaller preview/window rather than silently increasing limits.

## T11. Signed libraries, Apply versus Publish and upgrade

Apply saves an authorized document change. Publish creates a verified immutable
library revision and is a separate action. Publishing requires a clean confirmed
source, complete reachable definitions/styles/assets/code, export contract review,
version/revision metadata, exact imports and the existing authorized signer.
Private signing material is never available to the component graph or Wasmi guest.

New layout/style/source records require versioned schema/features and synchronized
native/server/browser admission. Bump state protocol when wire formats change;
release-control v1 remains the opening bootstrap. Fingerprints bind exported
semantic IDs, endpoint types, slots, behavior declarations, required style/layout
contracts and capabilities. Names and allowed appearance values may change without
changing semantic contracts; new unsupported capabilities cannot.

A valid signed candidate is not automatically hot-compatible. Use the existing
library compatibility predicate extended for the new contract. Validate/compile
the entire candidate, wait for the established connected-confirmation/gesture
boundary, then activate atomically. Failed signature, missing resource, unsupported
primitive, changed contract or resource overflow leaves the old admitted revision
active. Rollback retains exact known compatible code/library pins and never claims
to migrate arbitrary app state. Export removal/type change takes new admission.

Published code does not make every dependent app update automatically in this
proposal. Each app's existing exact lock and authorized adoption workflow selects
its candidate; the editor shows available revisions and impact. Do not add silent
floating dependency resolution or a new service to make publication convenient.

## T12. Implementation slices and qualification gates

| Slice | Deliverable | Exit evidence |
| --- | --- | --- |
| M0 Design model | This package, representative catalog, rapid prototype | Model transition tests; UI walkthrough; gaps recorded |
| M1 Core layout contract | Versioned Horizontal/Vertical Fixed/Hug/Fill, measurement, errors | Golden layout/cycle/overflow/hit fixtures; Frame resource bounds |
| M2 Working copy lifecycle | Blank/create/edit/Apply/cancel/undo, definition/instance distinction | Real authority conflict/receipt tests and local isolation checks |
| M3 Styles and composition | RFC 0037 resolver, modes, source-aware inspector, LabelledSlider | Paint proof, forwarding/binding/contrast/state tests; native/Web matching |
| M4 Dependency lifecycle | Impact index, blocked delete, retire and explicit replacement | Stale-index, in-use, override/route repair and signed export tests |
| M5 Base catalog | Qualified representative controls and flows | Each matrix row has actual evidence, not a thumbnail |
| M6 Signed release | Completed validated feature set | Signed closure, compatible rollback, focused live Frame/browser acceptance |

M1–M6 can be subdivided into reviewable commits but should not expose fake working
controls. Keep unavailable features honest. Wrap/grid, richer typography, assets,
effects, navigation and complex control APIs enter only through their own qualified
capability slices. Do not call M0's HTML prototype a shared Rust implementation.

Acceptance must include: blank component; composed LabelledSlider; definition
update with untouched and overridden instances; long/multiline content; Fill/Hug
cycles and overflow; state/mode previews; disabled and captured input; dependency
removal/replacement/retirement; stale Apply, lost reply, reconnect and authority
restart; signed update/rollback; no cross-Space mutation; aggregate resource and
actual present timing. Report each actual command, platform and evidence source.
Broad repository checks are warranted for the runtime contract cutover, not as a
pretend validation of this documentation-only proposal.

## T13. Questions resolved and remaining implementation decisions

Resolved: descriptive style fields; no global accent requirement; no constraint
solver; stable IDs and exact signed pins; no silent cascade delete; local working
copy with explicit Apply; Test sandbox; native shared compiler; Frame landscape
and large desktop priority; unsupported phone portrait; staged renderer breadth.

Before code: select the exact layout/style codec and field-address mapping,
confirm bounded mixed structural/property Apply implementation, specify text
measurement/rounding fixtures, and measure editor panel/resource budgets on Frame.
These are explicit implementation gates. They do not reopen the user-facing
lifecycle model or justify exotic fallback engines. If a gate fails, narrow the
slice and display the limitation rather than maintain two layout systems.
