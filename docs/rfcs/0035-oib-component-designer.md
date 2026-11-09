<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0035: OIB component designer and composable 2D controls

Date: 2026-11-03. Status: **D1, Frame Focus and bounded D2 implemented and live-qualified; remaining D2 and later stages proposed**.

The [D1 execution plan](../OIB_DESIGNER_D1.md) records the concrete milestone,
isolated worktree roles, integration gates and acceptance.
The [D1 qualification record](../experiments/oib-designer-d1-2026-11-03.md)
records the signed deployment, focused Frame/browser checks and limitations.
The [usability design](../OIB_DESIGNER_USABILITY.md) led to the completed
[Frame Focus editor](../OIB_FRAME_FOCUS_D1.md): readable tooling, a composition
tree, typed property drafts and measured Frame scrolling. Its
[shared-style qualification](../OIB_FRAME_FOCUS_STYLE_FOLLOWUP.md) records final
Frame/Web acceptance. [Desktop Studio and bounded D2](../OIB_DESKTOP_STUDIO_D2.md)
subsequently delivered persistent resizable panels and Insert/Edit/Delete/Reorder
of whole signed components within one admitted canvas. Nested/slot/definition
authoring, connection editing and D3/D4 remain deferred.

On 2026-11-09 the user selected the remaining D2 implementation and a clean
format break with fresh PoC data. The [D2 work record](../OIB_D2_COMPLETION.md)
separates the unreleased v2 document/protocol foundation from the remaining graph
compiler, authoring work and live acceptance. No old-data migration is required.

Extends [Live OIB](0029-live-oib-ui.md) and its
[current implementation](../OIB.md) into an application component designer.
Preserves [bounded instance lifecycle](0030-live-oib-instances.md),
[component geometry](0031-live-oib-component-geometry.md),
[restoration](0032-oib-restoration.md),
[instance reclamation](0033-oib-instance-reclamation.md), and
[authored instance defaults](0034-oib-authored-instance-defaults.md).
The user selected D1 for implementation on 2026-11-03. Its delivery stages keep
that foundation separate from later rendering and authoring work.

## Problem and intended result

Live UI currently edits a signed, fixed set of component properties and creates
instances of admitted factories. It demonstrates shared design changes and
instance customization, but cannot yet author a general object hierarchy or
compose all existing UI capabilities in one OIB document.

The intended tool has a hierarchy, a live canvas, and a modular inspector. An
author constructs reusable components from small primitives, places linked
instances in an application, and customizes their exposed properties. Definition
edits propagate through references; explicit overrides retain their scope.
The designer uses the same layout, renderer, input, and validation as applications.

A representative result is a reusable settings panel containing labeled sliders
and toggles. Two panel instances can share structure and styling while retaining
different labels, bindings, and overrides. Editing the component must not replace
application values, lose stable identities, or redirect an in-progress gesture.

## Decisions and scope

- `OIBObject` is the fundamental instantiable, nonvisual object class.
- `OIBDocument` is the explicit document root. `O2DView` and future `O3DView` are
  separate visual branches of `OIBObject`.
- Focus this design on 2D. Reserve `O3DView`; do not implement an XR layout engine
  or treat the current projected 3D demo as that engine.
- Start graphics with `O2DShape`. Defer `O2DBitmapImage`, `O2DVectorImage`, image
  uploads, decoding, and asset-picker UI to a later RFC.
- Include `O2DTextArea` for multiline editing. Do not introduce `O2DLabel`;
  labels are ordinary `O2DText` objects.
- Keep buttons, toggles, sliders, dials, and touch pads small. Compose their
  appearance from children and shared appearance properties.
- Build cards, labeled sliders, breadcrumbs, checkboxes, segmented controls, and
  dialogs as reusable library components.
- Model nested references, stable override addresses, and slots from the first
  document format, even when the initial editor permits only a bounded subset.
- Preserve the existing single authority, signed execution surface, shared
  session, local interaction state, and draft/active publication model.

General scripts, new execution backends, arbitrary class-code loading, portable
compilation, general transactions, and CRDT acceleration remain outside this RFC.
Cross-Space component libraries, component subclass inheritance, prefab variants,
arbitrary vector import, and a general animation timeline are also deferred.

## Existing foundation and gaps

The implementation already provides shared component properties, authored
instance defaults, live overrides and causal clears, instance lifecycle,
geometry mappings, durable receipts, conflict diagnostics, and validated active
publication. These features are the starting point, not new acceptance claims.
D1 shipped state protocol v27 and the required `otrios-oib-designer-v1` package
feature, preserving release-control v1. Its separate OIB Designer companion
retains the original Live UI app's schema, signed factories and values.

The shared UI runtime has parent-local frames, pins and springs, row/column/wrap
composition, rounded surfaces, circles, paths and triangle geometry, admitted
font rendering, clipping, responder bubbling and pass-through, scrolling,
text editing, chunked long documents, sliders, dials, touch pads, and series plots.
Availability in a demo or scene does not mean a capability is already a general
editable OIB property.

Several capabilities are currently selected by distinct `Presentation` variants,
including OIB, Editor, TextDocument, and TouchPad. Combining them requires
per-object admitted behavior and resource descriptors rather than stacking
mutually exclusive presentation modes. Preserve their existing implementations
while moving ownership to composable objects in separately qualified stages.

General content measurement, rich attributed text, arbitrary affine transforms,
general image rendering, and generalized accessibility semantics require further
work. Do not present these proposed controls as already implemented.

## Object and document model

Class names describe serialized authoring contracts, not a requirement for Rust
inheritance. Runtime implementations may use composition and a bounded class
registry. A signed class descriptor declares its base contract, properties,
child rules, rendering behavior, and connections. Mutable documents cannot add
executable class implementations.

`OIBObject` has a stable identity, class identity, display name, declared property
values, and one ownership parent, except for the document root. It can be
instantiated without geometry. Nonvisual controller/configuration objects can
appear in the hierarchy, but controller execution remains subject to the
application's existing admitted behavior contract.

`OIBDocument` owns document metadata, component definitions, document-owned
objects, resource references, and an explicit presented-root reference. It has
no layout frame. Initially one document presents one `O2DView` root. Definitions
are templates and are not painted just because the document owns them.

`O2DView` adds parent-local geometry, ordered visual children, layout, appearance,
visibility, clipping, and input participation. Ownership and visual containment
are distinct relationships: each view has at most one visual parent, and a
nonvisual child never consumes layout space. Both relationships must be acyclic.
The inspector exposes the distinction rather than silently reparenting objects.

`O3DView` is reserved as another branch of `OIBObject`. A future RFC must define
spatial units, transforms, constraints, input, and 2D/3D embedding. No implicit
embedding or coordinate conversion is provided here.

The logical records are:

```text
Document { id, schema, presented_root, definitions, objects }
ClassDescriptor { id, base, properties, child_rules, behaviors, connections }
PropertyDescriptor { id, type, default, constraints, editor, policy }
ComponentDefinition { id, root_node, nodes, exposed_properties, slots }
ComponentInstance { id, definition, placement, assignments, overrides, slot_children }
PropertyAddress { instance_ancestry, definition_node, property }
SlotDescriptor { id, admitted_child_types, cardinality, layout_policy }
Connection { source_address, declared_port, admitted_target }
```

These are explanatory records, not a wire encoding or new public Rust API.
Canonical schemas, discriminants, defaults, bounds, and test vectors must be
specified before implementing each format extension.

## References, identity, and overrides

### Definitions and instances

A component instance references a definition in its admitted document. It does
not copy the definition into an independent mutable subtree. Nested component
references form an acyclic dependency graph. The runtime derives the expanded
view while retaining the reference boundaries and original property identities.

Document, class, definition, template-node, instance, property, slot, and port
identities are stable and distinct from names and compact renderer indices.
Names are editable labels or scoped lookup conveniences. Rename, reorder, and
layout changes do not allocate replacement identities for surviving objects.

The resolved identity of an inherited node is its complete instance ancestry
followed by its definition-node identity. Two instances of the same definition
therefore never share an override address. Repeated nested instances include
each nested instance identity in that ancestry. Local selection, focus, and
semantic mappings use that resolved identity.

New authority-owned identities are monotonic and never reused. Renderer slots
remain reusable after old active references and gestures release them. Duplicating
an instance creates a fresh outer identity and fresh identities for owned slot
objects, remaps internal references, and copies explicit overrides while retaining
definition links and admitted external app bindings. No application data is cloned.

### Exposed properties and effective values

A component exposes a small typed public surface, such as `label`, `accent`,
`enabled`, and `value`. Exposed properties map to internal properties or typed
forwarding connections. Normal application composition edits this surface;
definition editing reveals internal children. Advanced nested overrides must
still use admitted stable property addresses.

Preserve RFC 0034's effective literal-value precedence, highest first:

1. An explicit live instance override at the addressed occurrence.
2. An authored instance assignment, including a signed factory-specific default.
3. The live value on the shared component definition.
4. The signed component default, falling back to the declared class default only
   where the component has not supplied one.

An absent live override inherits; assigning the same value as the inherited
value still creates an override. Reset causally clears only that live override.
It reveals the authored assignment when present, so a Brightness label does not
become Volume. Reset UI defaults clears live instance customization and restores
signed definition defaults while preserving authored assignments and app values.

Resolve nested boundaries outside-in. A declared forwarding connection supplies
the nested property's authored-assignment source from the resolved outer
property; it is not a copied snapshot. A live override targeting that nested
occurrence takes precedence there. Show this chain in the inspector, including
when a nested override masks a public outer property. Reject forwarding cycles
and conflicting mappings to one source property.

A property has one typed source: a literal, exposed-property forwarding, or an
admitted app-state binding. Evaluate bindings after resolving their source
declarations. The resolved display value is not another mutable property layer.
Editing a default cannot silently disconnect a binding. Changing a source requires
an explicit admitted connection operation; D1 keeps source connections signed.

Placement belongs to the instance's containing layout. Moving a card does not
rewrite its definition's internal design size, pins, or child positions. The
inspector separates placement properties from internal component properties.

### Slots and structural ownership

Slots declare accepted child types, bounded cardinality, and layout policy.
Instance-owned slot content is distinct from inherited definition children.
The hierarchy may expand inherited content for selection without materializing
copies in the stored document.

From D2, instance structural customization is confined to declared slots and
admitted instance roots. Deleting or reparenting an inherited definition child
from an instance is rejected. Edit the definition instead. Definition structure
editing arrives in D2; arbitrary inherited-child removal overrides and detach
remain deferred.

Deleting a definition or property that still has references requires an explicit
repair in the same admitted structural edit, or fails validation. Replacing a
referenced component preserves only explicitly compatible property/slot identities;
there is no name-based override guessing. Missing references and obsolete
overrides appear as bounded issues, never as silent reassignment to another node.

## Shared 2D properties and layout

All 2D classes share property groups instead of independently implementing
conflicting styling systems:

| Group | Contract |
| --- | --- |
| Placement | Parent-local frame, design size, pins/springs, alignment and per-axis sizing policy |
| Sizing | Fixed, fill available space, or fit content; minimum/maximum; explicit intrinsic-size source |
| Layout | Absolute child placement or row/column/wrap arrangement, gap, padding and alignment |
| Appearance | Fill, border color/width, corner radius/profile, and visibility; unsupported capabilities are not exposed as working controls |
| Clipping | Explicit content clipping, independent of the painted corner shape |
| Input | Normal participation, ancestor bubbling, decorative pass-through, or blocking cover; control-specific action behavior |
| Semantics | Accessible name and class-appropriate role, value, checked/disabled state, and focus policy as host support is added |

Each axis has one sizing policy; pins and layout-managed sizing must not compete
for the same dimension. Measure content, apply min/max constraints, then arrange
children in a bounded deterministic pass. Reject cyclic dependencies, such as an
unbounded fit-content parent whose only size source is a fill-parent child.
Absolute-positioned children contribute declared finite extents where fit-content
is admitted. Reflow and text measurement use shared logical font metrics.

D1 exposes existing fixed dimensions and pins/springs, existing bounded
composition, and explicitly declared scroll extents. General intrinsic measurement
and fit-content belong to D3. Unsupported policies remain unavailable in the
editor until qualified; do not substitute a different policy silently.

### Panels and scrolling

`O2DPanel` supplies a rounded surface, padding, and optional horizontal/vertical
scrolling. Keep surface painting, content layout, and scrolling separable internally
so an unstyled view or text document can reuse scrolling.

Viewport size and content size are separate. Support these intended cases:

- A viewport fills its parent; content width follows the viewport and content
  height fits a column of children.
- A compact card fits its content until a maximum height, then scrolls.
- A drawing surface has explicit content dimensions exceeding both viewport axes.

An edge fade is an overflow affordance, not a general gradient fill. Show a fade
only where undisplayed content remains, with configurable extent and color.
Its overlay explicitly passes input through. Edge fades are delivered with D3;
they do not block D1. General decorative gradients remain a rendering extension.

Scroll offsets, momentum, hover, focus, and gesture state stay local. Preserve
offsets across compatible property edits and clamp after content shrinks. Painting,
hit testing, and semantic bounds use the same transforms and clips. The visible
portion of a clipped control remains clickable; modal barriers remain blocking.
Prefer scrolling to Previous/Next controls. Any bounded paging exception needs a
documented scene/control/layout-byte/coordinate constraint and a focused test.

## 2D control set

### Shapes and short text

`O2DShape` is the first graphics primitive. The target vocabulary includes
rectangle, rounded rectangle/superellipse, ellipse, stroked polyline, and bounded
filled triangle geometry. Inspectors expose the applicable geometry, fill, stroke,
and corner controls. D1 uses already supported geometry, including square-frame
circles; arbitrary ellipses, stroke joins/caps, and additional paths require their
own supported contracts. No SVG/PDF parser or unrestricted path language is implied.

Shapes cover current geometric icons and the Components touch-pad grid, crosshairs,
and marker. They can be driven by admitted value expressions. An icon can itself
be a reusable shape component; it need not become an imported vector asset.

`O2DText` is short read-only text with a shared attributed-text model as the
longer-term target. Span properties include font family, size, weight, italic,
color, underline, strikethrough, tracking, and superscript/subscript. Bold is an
editing shortcut for a weight, not a contradictory second weight property.
Script styling combines baseline displacement and size while participating in
line measurement. Paragraph properties include alignment, line height, wrapping,
and truncation where applicable.

D1 supports the existing admitted font, uniform scale/color, and plain content.
Attributed spans, additional faces/weights, paragraph styling, and typographic
measurement are D4. Preserve explicit limits on glyph coverage, shaping, and bidi;
accepting Unicode input alone does not provide complete text rendering support.
There is no `O2DLabel` class or separate label text engine.

### Activation and value controls

| Class | Behavior and inspector additions | Composed appearance |
| --- | --- | --- |
| `O2DButton` | Enabled state, focus/accessible name, admitted activation action; capture on press, activate on release inside, cancel otherwise | Child text/shapes; shared fill, border and corner properties |
| `O2DToggle` | Boolean binding, checked/disabled semantics and change action; whole-view hit region | Child shapes/text react to on/off state; no prescribed switch graphic |
| `O2DSlider` | Numeric binding, finite min/max, positive step, orientation and change action | Track, fill, thumb and readout are children |
| `O2DDial` | Numeric binding, angle mapping, step, clamp/wrap policy and change action | Ring, ticks and indicator are children |
| `O2DTouchPad` | Two-axis numeric binding, coordinate mapping and down/move/up/cancel lifecycle | Background, grid, crosshairs and marker are children |

Button and toggle hit regions encompass the whole view rectangle, clipped by
ancestors, regardless of empty space or rounded paint. Decorative children do
not reduce that region. Nested active controls follow the deepest admitted
responder and normal ancestor rules; they do not also activate the outer button.
This contract must be tested against current shape-based hit testing rather than
assumed to follow from painting a rounded surface.

Pressed, hover, focus, and drag feedback are local state. Component authors can
map them to child appearance through admitted state selectors. Authored state
styles are document data; the current pressed/focused value is not shared data.
Forced states in a designer preview never mutate the live application's state.

Keep touch-pad fading ink separate from the two-axis control. Reuse the existing
ephemeral ink/timing path when that behavior is admitted; do not persist every
stroke sample as an OIB property edit. Initial slider/dial/pad controls expose
existing supported mappings; extra orientations or policies are later extensions.

### Text entry and long documents

`O2DTextField` is single-line editable text. Its inspector supplies the admitted
text binding, placeholder, byte/length bounds, secure entry, input purpose,
capitalization/autocorrection hints, submit behavior, and validation feedback as
each capability is supported. Standard input purposes are default, email, URL,
and decimal pad. Keyboard choice is a hint, not validation. Decimal validation
must declare sign, separator/locale, and accepted intermediate editing forms;
convert to a numeric app value only through an explicit admitted action.

`O2DTextArea` shares the editor behavior and binding model but supports multiple
lines, wrapping, vertical scrolling, and an explicit newline-versus-submit policy.
It is part of the planned set. It edits plain text initially; a rich-text editor
is not implied by read-only rich text support.

Preserve browser input/textarea IME and native software-keyboard integration,
grapheme-safe edits, UTF-16 host offset conversion, local caret/selection/focus,
and composition-aware activation. Existing atomic text registers remain atomic;
this RFC does not introduce a collaborative sequence-text CRDT. Reusable editor
behaviors arrive in D3, with input-purpose adapters qualified there.

`O2DLongText` displays a read-only document using the same attributed content
model as `O2DText`. Paragraph styles include alignment, indentation, spacing, and
line height. Keep source/document resources separate from short OIB property
cells. Reuse chunked layout and visible-range rendering; do not expand the whole
document into scene text nodes or raise short-property limits to hold it.
Plain long documents become composable in D3; rich spans/paragraphs arrive in D4.

### Layout, plots, and library components

`O2DStack` describes row/column layout; a wrap option admits wrapping where
supported. Inspector controls include gap, padding, alignment, preferred/minimum
sizes, and flexible axes. Keep these as layout contracts over existing views;
do not add a second layout engine. General fit-content measurement remains D3.

`O2DPlot` exposes bounded series or timed-series bindings, ranges, stroke, and
plot bounds. Grids, markers, axes, and text can be composed children. Existing
path rendering is reusable, while a general reusable plot behavior is D3 work.

The initial component library should demonstrate `LabeledSlider`, a shape-based
toggle/checkbox, a settings panel, and an ordinary text button. Cards, breadcrumbs,
segmented controls, and dialogs follow as compositions. Modal components must
retain a blocking backdrop and focus behavior; decorative pass-through is not
a substitute for modal input handling. New primitive behavior needs a concrete
interaction that composition cannot express.

## Designer interaction and modular inspectors

Use three independently usable, scrolling areas:

1. **Hierarchy:** explicit document root, presented root, definitions, nonvisual
   objects, linked component instances, inherited nodes, and owned slot children.
   Show origin/override/conflict indicators and distinguish selection from editing
   a definition. Expansion and selection are local.
2. **Canvas:** the resolved runtime preview with selection and placement tools.
   Show component boundaries, clip/overflow inspection, and viewport profiles.
   Selection outlines and resize handles must not become application content.
3. **Inspector:** identity, placement/layout, appearance, class-specific behavior,
   exposed component properties, connections, and diagnostics. Show the selected
   scope explicitly: definition, instance, or nested occurrence.

Begin with selection and numeric property editing. Direct manipulation, snapping,
alignment guides, multi-selection, and richer canvas tools can follow without
changing persistent property identities. In a narrow viewport, panels may collapse
or switch locally; this does not alter the authored application layout.

Property descriptors declare stable identity, value type, default/source policy,
constraints, units, inspector group, display label/help, supported editor, and
whether literal overrides or bindings are permitted. Labels and help text are
not identity keys. Reuse generated boolean, numeric, color, enum, text, object,
and connection editors; add specialized editors only where needed. Unsupported
editors provide a diagnostic rather than writing an approximate value.

Assemble sections from class ancestry and component descriptors. `OIBObject`
needs only identity and declared properties. `O2DView` adds shared layout and
appearance; `O2DButton` adds a small interaction section. A `LabeledSlider`
instance shows its exposed surface first, with internal inspection available.

Each property reports both origin (class/component/authored/live) and source
kind (literal/forwarded/bound). Conflicts and inactive drafts are separate status.
Offer per-property reset, inspect-source/definition, and explicit conflict
resolution. Show both pending draft and currently active values when they differ.
Promoting selected overrides into a shared definition is an explicit later
operation with an affected-instance preview; ordinary inspector edits never
silently publish back to a component or installed package.

Maintain a clear selection/edit mode and interaction-test mode so selecting a
button does not accidentally run its application action. Preview document edits
remain unconfirmed until admitted. Input testing uses the confirmed graph and
admitted actions; tests that modify app values use the isolated demonstration
collection. A preview cannot execute an invalid draft or bypass grants.

## Mutation, publication, and recovery

Keep three ownership domains separate:

| Domain | Examples | Update path |
| --- | --- | --- |
| Authored/live UI | Definition properties, instance overrides, hierarchy and slots | Space-authorized OIB operations |
| Application content | Volume, enabled value, document/editor content | Existing admitted app operations |
| Local interaction | Selection, scroll, caret, pressed state, gestures and preview | RAM-only device state |

D1 retains typed causal property batches and their conflict semantics. Disjoint
property edits merge; conflicting assignments remain in draft and keep the last
valid active graph. Reset remains a causal clear, not a write of today's default.
Persist accepted state and its receipt before publishing/acknowledging it.

D2 adds structural operations using an authority-serialized edit batch with an
expected document structure generation. A batch identifies parents, stable
insertion anchors, slot ownership, and targets explicitly. Check its expected
generation before allocation; a stale batch is rejected for rebase without
partial mutation. Two simultaneous moves do not produce two parents or an
implicit order chosen by display name. Property causality is still checked
independently; a structural batch cannot overwrite unrelated property registers.
This is one collection's bounded edit protocol, not a general transaction API
or a claim to have implemented a tree/sequence CRDT.

Validate the complete candidate closure: references, cycles, types, sources,
ownership, geometry, input semantics, expanded resource use, and admitted actions.
Invalid structural batches do not publish partial trees. Accepted property
drafts may remain conflicted; every viewer continues using the same active root.
Detailed structural encoding and repair operations are required before D2 coding.

Code and designer use the same admitted operations and authorization. Space
Admins edit UI initially; viewers receive active UI without private drafts.
Mutable connections may only select signed classes, fields, actions, and effects
already admitted for the binding. D1 cannot reconnect app fields. No edit can
introduce executable bytes, grants, authentication identities, services, or
cross-Space access.

Compile candidate layout, retained resources, semantic bounds, and responder
mappings together. While connected, wait for confirmed app edits and finished
gestures/composition before geometry or connection activation. Coalesce pending
revisions and preserve compatible focus and mount identity. If a later structural
change removes a focused/captured object, cancel its local interaction safely
at the activation boundary; never retarget it to a recycled renderer index.

Recovery must reproduce canonical draft/active graphs, receipt outcomes, and
identity allocation across an authority restart. Slow readers retain existing
bounded resync behavior. Reclamation follows RFC 0033 and must retain objects
needed by either draft or active state; do not discard causal context to make room.
Document undo is deferred. A future undo must be a new causally scoped operation,
not a restore of an old aggregate over other participants' edits.

## Limits, format evolution, and deployment

D1 starts within the existing ceilings: 16 definitions, 64 authored objects,
64 editable cells, 16 changes per property batch, 64 KiB draft data including
causal metadata, 128 KiB compiled layout, 512 expanded scene nodes, and depth 16.
Retain existing control, coordinate, actor/context, message, and receipt limits.
The inherited graph and every occurrence count toward expanded limits.

Where D1 reuses O2 allocation, retain six live allocated instance roots and ten
resident descriptors. Nested occurrences inside a signed definition derive their
identity from ancestry; they are not separately allocated roots. Slot-owned live
objects consume the applicable object/allocation budgets. Preflight the example
against all limits; a small definition cannot authorize unlimited expansion.
If the example cannot fit, narrow it or propose measured budget changes explicitly.

Later stages must specify additional source-byte, span/paragraph, path-point,
layout-work, and cache limits before implementation. Viewport clipping alone
does not bound document parsing or component expansion work.

D1 supplies the required package feature, canonical encoding and reference
vectors; the [contract](../OIB_DESIGNER_CONTRACT.md) records its attachment and
stable cell lowering. Bump the then-current state protocol whenever its wire
format changes, with matching browser/native/server identifiers. Preserve
release-control v1, the shared WebSocket, and opening current-release hello on
every reconnect. Obsolete state decoding must not block release discovery.

An existing v26 document is retained only through a tested explicit migration
from admitted predecessors, preserving identities, authored defaults, live
overrides/conflicts, app bindings, and values. Otherwise keep the old document
usable on its compatible runtime or report incompatibility explicitly. Do not
silently reinterpret/reset it. Assign protocol/feature names in the implementation
change rather than reserving a speculative version in this design.

Continue isolated Live UI development until portable data export/replacement
includes the complete OIB namespace, schema/seed bindings, references, and conflict
metadata. Current OIB transfers remain explicitly unavailable. Do not widen to
existing applications while claiming their exports are complete without UI data.
Signed app sharing remains immutable code/seed sharing, distinct from live data.

Deploy each completed runtime increment through the existing signed-release
verification workflow and keep a compatible signed code rollback. Code rollback
must not restore old application data. Preserve the established policy allowing
a disconnected client to discard unconfirmed RAM edits when upgrading; do not
add durable/offline edit migration as an incidental requirement.

## Delivery stages and acceptance

Only a separately selected stage is implementation scope. These are proposed
increments, not claims of completed functionality.

### D1: document foundation and a useful component inspector

Implement signed class/property descriptors, explicit document/root ownership,
nested component references, stable occurrence addresses, exposed properties,
authored defaults, and slot declarations. Use fixed signed component structures
and connections. Permit live property editing and existing bounded root-instance
creation/duplication/removal. No arbitrary hierarchy reparenting, definition
structure editing, source reconnection, or user-created slot content in D1.

Provide the hierarchy, local selection, modular inspector, and runtime canvas.
Use `OIBObject`, `O2DView`, panel, existing shapes/plain text, button, toggle,
slider, and existing layout/scroll policies. Restrict inspector choices to
implemented, admitted capabilities. The exact wire schema and typed addressing
API are part of D1 review before runtime coding.

The acceptance fixture has a nested `SettingsPanel`, reusable `LabeledSlider`
and toggle components, and two panel instances with signed distinct app bindings.
Use at least one factory-specific authored label and one explicit live override.
Required focused evidence:

- Definition accent/layout changes reach both panel instances except explicitly
  overridden occurrences. An inner override affects only its full instance path.
  Reset reveals the authored label or forwarded value as appropriate.
- Rename/reorder in separately authored compatible fixtures preserves occurrence
  identities and override targets. Parent placement changes leave child design
  properties intact. D1 does not claim live structure editing from these fixtures.
- Button/toggle whole-view hits, clipped visible regions, decorative pass-through,
  modal barriers, and ancestor bubbling behave on both hosts. Editing mode does
  not accidentally activate application controls.
- Two clients' disjoint property edits merge; same-property conflicts keep the
  exact active graph; resolution and causal reset converge. Invalid cycles,
  mappings, types and expansion overflow fail without replacing active UI.
- Repeated create/duplicate/remove uses fresh identities and bounded reusable
  resources. Existing live overrides and app values survive confirmed updates,
  reconnect and authority process restart. Delayed removed-object operations
  cannot affect a new occurrence using a recycled rendering slot.
- On Frame and a deployed browser client, a live data-only design edit propagates
  without code reload; held slider input retains its target and value. Record the
  release, commands, observed results, propagation timings and code rollback.

### D2: bounded hierarchy authoring

The [Desktop Studio and bounded D2 milestone](../OIB_DESKTOP_STUDIO_D2.md)
delivers the selected first slice: an existing-factory/Power palette and
Insert/Edit/Delete/Reorder of whole signed components within one admitted canvas.
The remaining D2 scope below, including slots, nested structure, definition edits
and connections, is deferred.

Add the insert/component palette, root/slot placement, owned-child reorder,
definition structure editing, and explicit source connections within the signed
edit surface. Specify structural generation checks and repair operations first.
Use numeric placement editing initially; direct canvas manipulation may follow.

Acceptance includes nested insert/move/remove, slot type/cardinality rejection,
referenced-node repair, stable overrides after rename/reorder, simultaneous move
rejection/rebase, durable structural receipts/restart, unchanged app values, and
safe local focus/capture cleanup. Arbitrary inherited-child structural overrides
and component detachment remain deferred.

### D3: composable view behaviors and measured content

Make scrolling, plain long documents, `O2DTextField`, `O2DTextArea`, dial, touch pad,
and plot behaviors coexist per object within a document. Add bounded intrinsic
measurement, fit-content policies, and overflow edge fades. Reuse existing retained
text, input, ink, and plotting implementations instead of adding parallel engines.

Acceptance places multiple behaviors in one screen, checks independent scrolling,
resize/reflow and input capture, and exercises browser IME/native keyboard,
multiline newline/submit, each supported input purpose, and content shrinking.
Reject sizing cycles and resource exhaustion. Show that long-document scrolling
does not rebuild all text or persist viewport offsets as shared UI data.

### D4: attributed typography and library refinement

Implement the bounded shared span/paragraph model, supported fonts/weights and
script positioning, rich `O2DText` and `O2DLongText`, and richer library components.
Specify source mutation granularity before making attributed content live-editable;
do not accidentally introduce a rich-text sequence CRDT. Qualify measured wrapping,
span/paragraph boundaries, baseline metrics, native/browser agreement, chunk reuse,
font-resource admission, and size/work bounds.

Bitmap/vector image classes, import, uploads, content-hash deduplication, and
asset selectors are a separate later RFC. That work must preserve immutable
original/derivative identities, complete-closure publication, and Space access
rules; a SHA-256 asset reference never grants access by itself.

## Alternatives and tradeoffs

Copying component trees would simplify initial editing but lose reliable
definition propagation and nested override scope. References are the chosen model.
A giant widget catalogue would hide reusable structure; composed library components
keep visual customization inspectable. Specialized behavior remains appropriate
for input, text layout, and data visualization where shapes alone are insufficient.

Keeping every existing presentation demo as a special document mode would avoid
short-term refactoring but prevent mixed interfaces. Per-object behavior is the
target, delivered after the bounded hierarchy/inspector foundation. Retaining
fixed signed structure in D1 gives useful component editing before introducing
structural concurrency and general authoring.

## References and ownership

Borrow object graphs and declared connections from
[Apple Interface Builder](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaFundamentals/CommunicatingWithObjects/CommunicateWithObjects.html),
visible instance customization and apply/revert from
[Unity Prefabs](https://docs.unity.com/en-us/engine/6000.0/manual/working-with-gameobjects/prefabs/override/prefab-instance-overrides),
exposed properties and slots from
[Figma](https://help.figma.com/hc/en-us/articles/5579474826519-Explore-component-properties),
and inspectors driven by actual component contracts from
[UXPin Merge](https://www.uxpin.com/docs/merge/configuring-the-properties-panel/).
These are design references, not file-format compatibility commitments.

Runtime resolution, validation, layout, behavior integration, and tests belong in
`crates/otrios-core` under LGPL-3.0-only. Public contracts and SDK declarations,
app/component authoring, build tooling, and documentation retain Apache-2.0.
Preserve Linux hardware/driver ownership. This documentation change adds no
dependencies, unsafe code, services, deployments, identities, or trust roots.
