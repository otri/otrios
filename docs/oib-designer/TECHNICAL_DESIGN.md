<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer: technical design

Date: 2026-11-11. Status: **selected implementation target; build-out in progress**.
Follow [IMPLEMENTATION.md](IMPLEMENTATION.md) for the concrete slices, active goal,
source ownership and actual runtime validation. Current PoC data is disposable;
use a clean versioned cutover instead of migration or obsolete UI compatibility. This document is a
contract design; the implementation ledger identifies the delivered shell and
initial Auto Layout work separately from named styles and unfinished lifecycle work. Read [the package entry point](README.md),
[UX journeys](UX_JOURNEYS.md), [visual reference](../../DESIGN.md) and
[RFC 0037](../rfcs/0037-oib-design-system.md) together. The HTML prototype validates
selected design-model transitions; it does not substitute for native/Web runtime
admission, input, signing or performance acceptance.

## T01. Existing foundation and explicit gaps

| Concern | Existing implementation | New design work |
| --- | --- | --- |
| Canonical source | OIB v2 graph and optional typed Auto Layout records; semantic addresses, definitions/presets, instances and typed source connections | Richer component contracts and named-style references |
| Editing | Generation-checked structural EditBatch; independent causal literal registers | Coherent local working copy and explicit Apply across the new field types |
| Appearance | Definition-node defaults; compiled theme constants; limited live colors | Typed named styles, variable modes, source-aware full appearance inspector |
| Runtime | Shared Rust scene/layout/input, Wasmi component ABI 1; library-owned Slider | New generic layout compiler and incremental dependencies, additional admitted controls |
| Packaging | Verified exact signed library closure; compatible L1/L2 adoption and rollback | Contract fingerprints/capability admission for layout/styles/new exports |
| Editor shell | Responsive canvas/task dock replacing earlier Frame Focus/Desktop Studio profiles; retained panel scrolling | Consistent full lifecycle, impact review, catalog qualification and draft handling |

Sources of truth are `crates/otrios-interfaces/src/oib/{designer,graph,library}.rs`,
`crates/otrios-core/src/canvas/oib/`, `canvas/client/oib_designer*`,
`docs/OIB_D2_COMPLETION.md` and `docs/OIB_LIBRARY_L2.md`. `graph::Visual` currently
contains frame expressions, legacy ResizeRule and optional NodeLayout. Fixed
lengths use the existing frame expressions; sizing policies do not duplicate them.
Layout v6 admits this shared runtime model; state protocol v32 carries its source
edits. Source has Literal, Forward and AppBinding. StyleId and variable modes
remain unimplemented. Earlier pins/struts remain a historical/internal
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
  id, definition_ref, preset_origin?, owner, visual_parent, slot?,
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

### T02.1 Preset defaults and reusable-record lifecycle

Presets are insertion recipes: definition reference plus typed exposed-property/
style assignments. Insertion copies those expressions/refs into the new instance
once; later preset edits affect future insertions only. Optional preset_origin is
informational provenance, not a live dependency. Shared style/variable refs inside
the copied assignments still resolve normally. Updating selected existing instances
requires an explicit mapped candidate, never implicit propagation of preset edits.

New/duplicate/fork definitions, presets, styles and variables receive fresh stable
IDs in their authorized owner. Rename preserves identity. Retirement of a reusable
record affects insertion/picking only; current refs remain resolvable. Deletion is
blocked for live local refs unless repaired atomically, and exported-contract
removal still requires new admission. Duplicate styles preserve variable refs by
default; copying variables is an explicit ID-remapping option. See J25/J26 for
complete narratives. Concrete lifecycle metadata joins the versioned schema gate.

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
is supplied. Bounds apply to the outer border box. An empty container's Hug size is the
clamped padding/stroke extent: max(minimum, intrinsic content + padding + stroke),
subject to max >= min. A blank root with min-height 48, no stroke and vertical
padding 16+16 is therefore 48 high, not 80. Editor drop-zone text is excluded.

No layout property implies semantic interactivity. A 16-unit icon can be inside a
48-unit control target. Target enlargement must remain within its owning control
and cannot overlap unrelated targets. Hit testing uses the final clipped geometry.

The initial implementation uses `layout::NodeLayout` with per-axis `AxisSizing`,
`Sizing::{Fixed,Hug,Fill}`, explicit `Placement::{InFlow,Free}` and optional
`AutoLayout` container policy. Container clipping stays in `Shape::Canvas`; there
is no second clip flag. The compiler marks expanded reference roots in layout v6:
their authored metrics supply intrinsic measure, then the caller's allocation
sets their presented border box. Container flow belongs on a local wrapper;
linked references expose item sizing rather than rearranging imported internals.

### T03.2 Deterministic core algorithm

The compiler builds a dimension-dependency graph before allocating draw resources.
Measure intrinsic content bottom-up, allocate available container lengths top-down,
then place children. Work with the existing renderer's canonical layout units and
checked finite arithmetic. Layout v6 retains canonical finite f32 logical values
without integer/pixel snapping; fractional Fill fixtures fix that behavior across
serialization. Renderer rasterization owns final pixel coverage.
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

### T03.4 Visibility, opacity and interaction

Proposed source visibility is Visible, Hidden or Collapsed, independent of enabled
and opacity. Visible participates normally. Hidden retains measured layout space
but emits no paint/hit targets and cannot receive focus. Collapsed is excluded from
flow measurement/placement and emits no paint/hit targets; stable source IDs, app
values and instance overrides remain. Revealing it restores those identities.
Visibility changes never delete nodes or disconnect app bindings.

Changing to Hidden/Collapsed cancels that occurrence's captured preview gesture
and moves focus to the nearest visible enabled ancestor/control, otherwise the
canvas. Selection moves to its nearest visible owner; Layers still lists the node
with its visibility state so it remains editable. Removing a whole modal routes
through the modal lifecycle and restores its prior valid focus target. Do not
accidentally expose a background interaction during a partially updated scene.

Opacity affects paint alone; zero opacity does not implicitly remove layout or
input. This preserves explicit invisible modal/input barriers. Warn about an
invisible ordinary interactive control, and provide visibility or input policy as
the intentional fix. Decorative nodes continue to require explicit pass-through.
Hidden requires paint/input/semantic invalidation; Collapsed additionally requires
layout. Neither can use the existing Color-only paint proof. Visibility is a typed
source property, not an RFC 0037 color-state patch or a color variable. Its concrete
codec and focus/input conformance must be qualified before enabling its inspector.

### T03.5 Wrap and grid extension gates

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

Use one responsive canvas/task-dock composition on all hosts. There is no saved
Frame/Desktop profile or device-based layout branch. A single local active-task
value selects Library, Layers or Inspect in the dock; underlined tabs convey this
navigation. Edit/Test is a separately labeled segmented mode group and appearance
is a labeled segmented switch. Neither shares navigation state or emits document edits.

Available viewport width controls the canvas/dock allocation; available height,
chrome and software-keyboard reservation determine scrollable work height.
Resize must retain semantic selection, field drafts, per-task scroll anchors,
Test sample state, mode and focus when still valid. Measured preview sizes refresh
without recompiling source merely because editor chrome changed. Production must
separate editor viewport sizing from the chosen finite preview-parent fixture.

The prototype dock is 280–360px and its work height adapts to the viewport minus
measured chrome, bounded to 240–720px for inline rendering. These are study dimensions, not permission to alter current scene caps
or the production 48-unit touch target. Test smaller/wider sizes with the same DOM
and source to catch accidental state resets. Unsupported phone portrait shows a
notice while retaining local work; it is not a compressed authoring profile.

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
| M6 Live release | Completed validated feature set | Existing publisher path and focused live Frame/browser acceptance; no extra signature checks or rollback preparation |

M1–M6 can be subdivided into reviewable commits but should not expose fake working
controls. Keep unavailable features honest. Wrap/grid, richer typography, assets,
effects, navigation and complex control APIs enter only through their own qualified
capability slices. Do not call M0's HTML prototype a shared Rust implementation.

Acceptance must include: blank component; composed LabelledSlider; definition
update with untouched and overridden instances; long/multiline content; Fill/Hug
cycles and overflow; state/mode previews; disabled and captured input; dependency
removal/replacement/retirement; stale Apply, lost reply, reconnect and authority
restart; live update; no cross-Space mutation; aggregate resource and
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

## T14. B5 implementation checkpoint — 2026-11-11

**IN PROGRESS; not deployed.** The current implementation uses one live adapter
for confirmed state and a private local working copy. Its bounded OIB Apply admits
at most 16 operations and 8 KiB, stages source and causal changes together on a
private candidate, and publishes atomically after validation. The state wire
format is v33, while the opening release-control envelope remains v1. Undo
history before Apply is local and bounded to 64 KiB.

The working copy pins its epoch, confirmed baseline, structural generation and
exact causal contexts; new targets receive a seeded context. A durable per-writer
digest receipt supports identifying and reconciling an Apply. Candidate preview
uses the same source privately and creates a private virtual instance of the
selected definition with sample values. Semantic value projection does not edit
imported bodies. Edit/Test mode and local drafts describe editor state; neither is
a canonical source generation or evidence of a confirmed publication.

Local focused evidence currently covers Apply (11 passed), working choice (1),
preview/clip (12), workspace (6), focus (4), and layout (4). The integrated Designer suite passed 66 tests (one existing ignored timing diagnostic). One real-process macOS server/WebSocket test passed for authorization, mixed publication, lost reply, restart and replay; this is not Linux IPC or live Frame acceptance. Conditional post-Apply Undo, typed API/slots UI,
retirement/dependency lifecycle remain B5 gates. Apply/Discard/Stay leave confirmation passed three focused receipt/rejection/concurrent-edit/input-barrier tests. Full B6
styles, catalog qualification and live Frame/Web acceptance remain open.
Implementation entry points are `canvas/state/oib/apply.rs`,
`client/oib_working_copy.rs`, `client/oib_editor.rs` and `oib/graph_preview.rs`;
finish those gates and focused validation before the signed deployment workflow.

### T14.1 Live boundary and remaining gates

The initial B5 working-copy/Apply/leave slice is now live on Frame/Web at code
`a5b1e3a`, release `4faf7ae090027833d1a3773236b2a790928a9ba42adb43a18311f5e40295e3ab`.
Live pointer acceptance required a correction to outside-panel toolbar capture;
one action classification now serves both Down and matching Up validation.
Designer68 passed (one ignored timing diagnostic), guest14, Apply11, preview12,
leave3, real macOS process/WebSocket1 and browser session/update22 passed. Live
Frame/browser edits confirmed generations31/32 with no insertion for preview,
no private publication before Apply, and working Stay/Discard barriers.

This does not close full B5 or desktop/canvas visual qualification. Local history
must restore selection; post-Apply history is still conditional-work pending.
Typed API/slots, imported linking, dependency impact/retirement/replacement and
full style/mode resolution remain open. The 1920 browser observation initially
retained uniform fit of the 1280 scene; validate the settled viewport adapter.
Snapshot capture timed out and alternate browser access was unavailable, so
visual capture/human native touch/color acceptance remains unproven. See the
final IMPLEMENTATION.md restart list for authoritative next work.

## T15. B5 history, composition and viewport contracts

This follow-up is implemented, locally tested and published as commit `2f95205`;
see the final IMPLEMENTATION.md entry for exact release, focused Frame/Web
acceptance and restart paths. Full B5–B7 remain in progress.

An accepted Apply is one published history unit. Undo/Redo prepares a new private
inverse and uses the same explicit Apply boundary. The inverse guards affected
source fields and actual authoritative property post-versions; reading a later
coalesced replica's equal value never establishes proof. Only a known accepted
local inverse may reanchor an overlapping earlier guard. Rejections preserve the
history entry. Normal edits cancel pending history intent, even if a later local
Undo returns to identical bytes. Local and published history share 64 KiB.

State v34/control v1 adds ordered authoritative property versions to the durable
Apply receipt. Missing historical proof is gated. ClearDefault removes only an
authored fallback; causal register identity is independent. Review presents
Before/Applied/Current with bounded scrolling, fixed actions, modal input barriers
and a cached authored layout. Display summaries may be shortened with an explicit
character count; conditional canonical bytes are never shortened.
Open comparisons retain their captured source and input owner on incoming edits;
a status line identifies stale comparisons, and retrying Undo uses current source.
Unsupported fresh-ID graph restoration is a visible reason; no root rollback or retired-ID
resurrection is allowed. Full removal/repair and creation Undo→Redo remain gates.

An OIB-only transient identity reservation plan makes phase-ordered mixed batches
independent of the local order in which definition, preset and instance IDs were
created. Every new ID must be consumed once and all claims must exactly fill the
captured authority-counter interval. The plan is private and cleared before the
single publication. Imported Link choices use admitted exported roots through
canonical resolution, preserving exact revisions and immutable implementation
bodies. New component, Text, imported Slider and a new-node property edit can
therefore be one coherent Apply.

Signed layout v7 adds a reusable Blueprint viewport policy. Fit is the legacy
omitted default; Responsive is admitted only with its declared capability and
finite bounds. Designer opts into minimum 800×480. The browser supplies window
logical dimensions within the existing 8192 bound; the shared renderer resolves
both composed and compositionless responsive roots. UI entry/policy changes force
reconciliation even when cached desired dimensions match. Held gestures retain
their original input revision until scene extent commits. Existing Fit/Chat/Map
behavior remains; no application-name lookup or host profile controls are added.

## T16. Fresh identity restoration for conditional history

The next B5 slice reconstructs removed definition fragments through the existing
CreateDefinition/CreateNode/SetSlots/Expose/Preset commands. It does not restore
an old document root or re-admit imported implementation bodies. Retain only the
affected pre-edit graph fragments and explicit causal override values, charged
to the existing shared 64 KiB history budget. Keep structural comparisons against
the accepted post-edit source, and preserve the exact receipt boundary.

When preparing Undo/Redo, allocate one deterministic interval beginning at the
current authority high-water mark. Remap removed definition/node/slot/preset IDs,
ownership, visual child order, forwarding targets, slot hosts, reference paths,
and affected preset assignments. Surviving and imported identities remain exact.
Restore ownership before visual links, then presets and existing instance repairs.
The signed class/property and action/binding grants still govern the candidate.

Capture property alternatives before deletion, never resolved inherited colors or
app field values. A single explicit override on removed content is written to its
new semantic address. Obtain that address's initial causal context by staging the
structural inverse privately, then submit structure and overrides together in one
Apply. Prior conflicts require explicit review; they cannot be flattened into a
chosen value. Neither original retired addresses nor their causal registers are
resurrected. Remap local selection to the restored object before presenting it.

The captured pre-delete replica must also be proven current. Existing Apply
guards the structural generation, so a property-only publication can race ahead
of deletion. Retain restoration history only when the accepted UI generation is
exactly one after the captured base UI generation. A gap gives an explicit
unavailable-history reason; the post-delete replica cannot reconstruct what was
actually removed. This conservative check uses the existing receipt, including
equal-value causal publications, and adds no wire fields or implicit overwrite.

Validate the final inverse, including restored property changes, against the same
16-operation/8 KiB Apply envelope and source/scene/control limits. An oversized
inverse remains unavailable with a visible reason; splitting one history unit
into several publications is not permitted. T18 extends this contract to removed
owned occurrences. App-bound graph reconstruction and dependent repair retain
their explicit grant/admission gates.
See IMPLEMENTATION.md for actual implementation and acceptance status.

## T17. Component deletion impact and confirmation

Library exposes Delete component for a selected definition even when deletion is
blocked. The command opens a local review, not an executable deletion disguised
as an unavailable toolbar choice. Query the captured canonical candidate and its
exact admitted import closure, grouping named direct/transitive component uses,
instances, presets, API/slot/property connections, exports and fixed ownership.
Each known navigable use opens its semantic selection. State the document-scoped
scan boundary; incomplete, missing or cyclic source never proves zero uses.

Palette presets belonging directly to the definition are listed as removals in
the same candidate. Required presets, live/transitive uses, fixed shell ownership
and imported ownership block deletion. Local deletion does not remove a signed
library export. Retirement, replacement mapping and publication remain separate
capabilities; this report does not simulate them.

Keep component cancels the review. Confirm recomputes impact and final admission
against the exact current source, then stages one private definition/preset batch.
Global Apply remains the publication boundary. Deletion drops unapplied causal
values owned by that definition while preserving other working-copy edits; local
Undo retains the complete pre-delete command, including those values. Accepted
deletion uses T16 conditional fresh-ID restoration for published Undo.

An incoming source revision makes a captured review stale. Keep its rows, action
geometry and held pointer stable; disable confirmation/navigation and require an
explicit Check again. Do not automatically retarget the action under a held
pointer. A dirty working copy on older structural source remains blocked after
rescan, retaining its edits until the author resolves or discards them. All modal
paths gate background pointer, semantic, text and app input.
The shared dialog capture handles matching release, cancellation, drag scrolling
and foreign pointer IDs for both history and deletion reviews.

Dependency rows wrap into clickable blocks of at least 48 logical pixels. The
visible clipped part remains clickable and offscreen rows are culled. Scrolling
changes the local presentation overlay and retains the authored layout cache.
Reports scroll as one list when they fit the existing 8192-pixel content bound
at the supported minimum 800-pixel viewport. Only reports exceeding that bound
use bounded pages, preserving every reported row with bounded navigation IDs. Existing combined
512-node/128-control scene ceilings and the 96-byte semantic label ceiling remain;
long visible labels retain their full text, with an explicitly abbreviated
semantic label. No limit increase or new wire contract is implied.

## T18. Removed occurrences and owned slot contents

History retains only removed instance records, their placements, explicit app
connections, affected sibling groups, occurrence origins and authored override
values. The app field store is never copied into an inverse. Removed instances
must have a captured palette preset matching their component; that preset and
the relevant source relationships remain conditional guards.

Recreate instances through ordinary admitted Insert operations after any fresh
definition/node/slot/preset declarations. Assign implicit IDs in the actual
parent-before-child insertion order, even when an old child's numeric ID predates
its old owner. Remap slot destinations and object paths to the fresh owner, then
restore sibling ordering and explicit connections through existing signed grants.
Seed fresh override contexts privately and include the allowed restored values in
the same final Apply. Later edits to surviving app fields/properties are preserved.

A changed sibling group, missing/rebound prototype, unavailable slot, conflicting
old override or failed final admission blocks the inverse with review/reason;
there is no partial restoration or silent fallback to another container. Existing
16-operation/8 KiB Apply, 64 KiB history and resident/source/scene ceilings remain.
Mixed replacement batches can still hit the existing private staging capacity
before removed residents are reclaimed; no final or transient capacity is raised.

Selection is advisory local state. Map known recreated targets to fresh semantic
addresses; an unrelated obsolete selection does not invalidate a guarded data
inverse. The normal Designer selection validation supplies its valid fallback.
This never weakens property/source guards. Earlier history entries still naming
retired identities require an exact causal-version remapping before they can be
reanchored. Equal restored values alone cannot establish that proof.

Instance removal uses a typed occurrence target, distinct from component-definition
deletion. The management action is Remove..., with a Remove instance? review that
names the selected occurrence, all allocated owned descendants, their explicit
app connections and retained reusable definitions. Compute the closure from exact
slot ownership paths; remove descendants before owners, preserving source sibling
order independently of numeric IDs. Missing, cyclic or ambiguous source and fixed
signed occurrences block confirmation. An imported definition does not make an
otherwise editable allocated occurrence fixed. Final candidate admission checks
slot cardinality, grants, layout and unchanged operation/byte limits.

Opening or cancelling the review changes no authored values or private buffers.
Confirmation rechecks both current live and private source roots, recomputes impact
and admission, then stages one private removal batch. Discard raw editor buffers
and staged literal changes only for the removed closure; preserve unrelated edits,
definitions, styles and app field values. Local Undo restores the prior candidate,
including its staged values. Published Undo restores the removed confirmed state
with fresh identities under the guards above. The shared T17 modal capture,
staleness, explicit rescan, navigation and scrolling rules apply to this review.

The broader replacement/retirement journey remains open. See the latest
implementation record for actual tests and Frame/Web publication status.
