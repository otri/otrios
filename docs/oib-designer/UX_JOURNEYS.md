<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer UX journeys

Date: 2026-11-11. Status: **proposed interaction design**, not a deployed feature list.
Read this after [the package guide](README.md). The visual language is
[DESIGN.md](../../DESIGN.md); named styles follow [RFC 0037](../rfcs/0037-oib-design-system.md).
Stable journey IDs below are implementation and qualification references. Preserve
IDs when revising a journey; add subcases instead of renumbering the catalog.

## 1. Shared vocabulary and rules

A **component definition** is a reusable template with stable identity, an owned
hierarchy, declared properties, slots and actions. A **linked instance** refers to
that definition and can override its exposed properties. An internal node is not
an independent document instance. A **style** coordinates appearance fields; a
**variable** supplies a typed value that can vary by Light/Dark mode. A **preset**
is a named set of initial assignments, not a hidden copied definition.

The Designer uses these boundaries everywhere:

- **Working copy:** local proposed edits against one confirmed base revision.
  The canvas previews these edits immediately when valid. Other editors and live
  applications continue to see the confirmed document. The badge says Unapplied.
- **Apply:** validate and submit one bounded transaction against the captured
  document epoch/generation. The UI distinguishes Sending, Confirmed, Rejected
  and Outcome unknown. A local preview is never labeled Saved or Published.
- **Publish library:** build, validate and sign an immutable library revision from
  confirmed source. It is separate from Apply. Activation/adoption by applications
  is another visible, policy-controlled step.
- **Test mode:** interactive preview uses declared fixture state. It never writes
  production app state, publishes a style or sends an external action.
- **Inspect scope:** Definition, Instance, or imported read-only source is always
  visible near the selection name. A source change is explicit; selection alone
  cannot silently change editing scope.

The current product already supplies canonical D2 graphs, semantic property
addresses, typed connections, atomic structural edits and confirmed color-only
paint optimization. The complete local working-copy transaction experience,
named styles, generalized layout controls, dependency repair and catalog below
are proposed extensions. They must reuse those contracts rather than create a
second mutable layout tree. Existing 24 KiB structural source and 64 KiB combined
resident authored/causal budgets remain constraints until separately changed.

No workflow depends on hover, long press, drag precision, keyboard shortcuts or
opening a context menu. Those can accelerate desktop use; visible commands must
remain available on touch. Lists scroll rather than becoming Previous/Next pages.

## 2. Workspace and navigation contract

**One responsive workspace:** a canvas and a right-hand task dock share the same
information architecture at every supported width. There is no Frame/Desktop
profile selector. Library/Layers/Inspect are underlined navigation tabs selecting
the dock content. Edit/Test is a separate segmented interaction-mode control;
Preview appearance is a labeled local-context segmented switch. Commands and revision
status remain separate from these choice groups.

Target logical Frame landscape is 1280×800, with larger desktop layouts as another
qualification target. Width changes grow/shrink the dock and canvas automatically,
with toolbar groups wrapping as needed. Preserve active task, selection, field
drafts, sample values and scroll anchors. Phone portrait remains unsupported;
show a readable notice without discarding work. Software keyboard reservation
scrolls the active field into view without compressing all panels or shrinking text.

Canvas background and document/component background are visually distinct.
Selection outlines and alignment guides pass input through. Edit mode selects;
Test mode operates. Pressing a control on the canvas in Edit selects that control,
without also toggling or sending its action. Escape/Done leaves nested edit scope
before leaving the component. A breadcrumb shows the full nested definition path.

## 3. Journey catalog

| ID | Journey | Primary result |
| --- | --- | --- |
| J01 | Open the library/workspace | Correct source and empty/loading/error state |
| J02 | Create blank component | Valid minimum reusable definition |
| J03 | Insert/reorder a child | Explicit hierarchy and placement |
| J04 | Build LabelledSlider | Composed control with a usable API |
| J05 | Configure layout | Predictable Fixed/Hug/Fill and flow |
| J06 | Style, modes and states | Coordinated appearance with source visibility |
| J07 | Expose component API | Typed stable properties, actions and slots |
| J08 | Connect application data | Valid bindings without hidden business state |
| J09 | Modify existing definition | Deliberate shared change with impact view |
| J10 | Override/reset an instance | Local customization with inheritance |
| J11 | Duplicate, fork or extract | Explicit reuse/copy semantics |
| J12 | Remove a child/control | No dangling internal references |
| J13 | Delete unused definition | Safe removal of an unreferenced template |
| J14 | Delete in-use definition | Block or repair, never silent cascade |
| J15 | Retire/deprecate | Preserve existing users while discouraging new use |
| J16 | Apply, undo and redo | Atomic confirmed changes with clear history |
| J17 | Handle stale/conflicting work | Keep proposed edits and deliberate reconciliation |
| J18 | Test and qualify | Reproducible fixtures and clean Test isolation |
| J19 | Publish/update/rollback | Verified immutable library lifecycle |
| J20 | Recover/disconnect/restart | Honest durability and outcome reporting |
| J21 | Accessibility/input review | Usable touch, keyboard and semantic behavior |
| J22 | Change workspace/preview size | Responsive editor without authored mutation |
| J23 | Handle capability/resource limits | Actionable admission failures |
| J24 | First-use and catalog progression | Small complete flows instead of a tool tour |
| J25 | Preset lifecycle | Explicit insertion defaults, existing-instance semantics |
| J26 | Style/variable lifecycle | Create/fork/retire/delete with typed references |
| J27 | Visibility lifecycle | Hidden/Collapsed versus deletion and opacity |

## J01. Open the library and workspace

**Entry/preconditions.** The author opens Designer in an admitted Space. A previous
selection may exist locally. The author may have read-only permission, no editable
library, a library with no definitions, or a connection still loading.

**Steps and feedback.**

1. Show the Space/library selector and status before trying to restore selection.
   A loading skeleton must not look like an empty library. Keep navigation usable.
2. Resolve the selected library revision and document permission. An empty library
   shows “No components yet”, Create component and Import/library selection where
   supported. Do not display fake demo controls as authored content.
3. Restore the last still-valid component selection and active task locally.
   If the component was removed, select the library, explain that it is unavailable
   and offer the component list; do not redirect an old ID to a new object.
4. A read-only source shows View source and Fork to editable library, with editing
   controls visibly disabled and the reason adjacent to the source heading.
5. A real empty component canvas shows its root boundary and “Insert your first
   control” action. A library with no component selected shows a library-level
   empty state, not the same canvas state.

**Saved effects versus preview.** Opening/restoring selection writes no document
edits. Panel sizes, navigation and source selection are local preferences.

**Cancel/back/error.** Loading failure offers Retry and switch library. Permission
loss leaves a readable snapshot where policy allows and preserves the unsent local
working copy as unsent, without pretending Apply is possible. See J20.

**Acceptance.** Empty, loading, unavailable, read-only and removed-selection states
are distinct. Create is reachable on Frame without a context menu or keyboard.

## J02. Create a new blank component

**Entry/preconditions.** The author selects Create component in a writable library.
No saved identity is allocated until the creation transaction is accepted.

**Steps and feedback.**

1. Ask for component name and root kind. The first implementation defaults to a
   visual container; advanced nonvisual objects appear only when admitted.
2. Explain that a blank component begins with a vertical flow container, 16-unit
   padding, 8-unit gap, width Fixed 320 and height Hug with a minimum empty height
   of 48. It has no inserted controls, fake label, handler or application binding.
3. Show the empty root outline and a large Insert control action. Properties shows
   component identity/name, default style, Layout and the empty API section.
4. The author can rename, set root sizing, insert a child, or Apply the valid blank
   component. A blank root remains a legitimate reusable component.
5. Apply presents “Create [name]” and confirms its assigned stable identity. The
   component then appears in Library with a blank specimen thumbnail.

**Saved effects versus preview.** Before Apply the object is a local working copy
with temporary authoring identity. Accepted creation assigns durable identity
atomically. An empty visual root is still valid structured source, not a missing
root, dangling preset or zero-node invalid graph.

**Cancel/back/error.** Cancel creation discards the unsaved local candidate after
an explicit discard choice if it contains work. Duplicate names can be disambiguated
or renamed; names must not determine identity. Invalid names are inline errors.
Capacity failure leaves the candidate editable and shows which source budget fails.

**Acceptance.** Create blank → Apply → reopen preserves a valid empty definition.
Cancel allocates no durable definition. Nothing is implicitly installed in an app.

## J03. Insert, select, reorder and reparent a child

**Entry/preconditions.** A component root or valid slot/container is selected in
Edit mode. The Insert panel knows the intended parent and can explain incompatibility.

**Steps and feedback.**

1. Choose Insert. The panel groups Basic, Controls, Containers, Feedback, Navigation,
   Data and later capability-dependent families. Search can narrow an admitted list;
   unavailable entries explain the required capability and cannot be inserted.
2. Select a control specimen. Show its name, purpose, exposed properties and
   “Insert into [parent]”. The selected parent/slot is visible during confirmation.
3. Insert creates a local linked child after the selected sibling, or at the end
   of the selected container. Default insertion uses flow, never arbitrary X/Y.
4. Select the new child in Layers and canvas. Focus the first useful property
   without opening the touch keyboard automatically.
5. Move earlier/later is always available. Desktop drag-and-drop previews a precise
   insertion line; touch uses Move → parent and position. Neither changes the saved
   graph until a valid drop/choice and then Apply.
6. Reparenting previews the new layout and enumerates affected bindings/slots before
   committing. Same-parent reorder keeps identity and overrides. Cycles and invalid
   slot cardinality are refused before a local invalid graph can replace the preview.

**Saved effects versus preview.** Insert/reorder/reparent mutate the local candidate.
Apply publishes one coherent hierarchy with stable surviving identities and a new
identity for the inserted occurrence. Sample/Test values are not copied as defaults.

**Cancel/back/error.** Closing Insert without choosing adds nothing. Escape cancels
an active drag. Invalid parent/drop shows the reason at the destination. If a
candidate exceeds the admitted scene/source budget, keep the existing preview and
show the rejected operation, not a truncated half-inserted child.

**Acceptance.** Button, Text, Slider and nested container insertion work with touch
and keyboard; reordering preserves bindings. Descendant-as-parent is rejected.

## J04. Compose LabelledSlider from a blank component

**Entry/preconditions.** Start with J02 named `LabelledSlider`. The base library
contains Text, Slider and flow container controls; arbitrary arithmetic expressions
are not assumed. The representative exercise uses 0–100, step 1 and sample 40.

**Steps and feedback.**

1. Select the root: vertical flow, width Fill within its parent when instantiated,
   height Hug, padding 16 and gap 8. In the standalone specimen provide a finite
   preview parent width so Fill has a defined available width.
2. Insert Text. Rename its layer `Title`; set its authored text to “Volume”, width
   Fill and height Hug. Explain that changing layer name does not change text.
3. Insert Slider after Title. Rename its layer `Slider`; set width Fill, height
   Fixed to the admitted accessible control height, minimum 0, maximum 100, step 1,
   authored value/default 40. The control uses `slider.default`.
4. Select API → Expose property. Expose Title.text as `label` (Text), and Slider's
   value, min, max, step and enabled under descriptive names with declared bounds.
   The inspector shows target paths so forwarded values are not invisible links.
5. Expose Slider.valueChanged as the composite's `valueChanged` action/outlet with
   its typed numeric payload. Preserve the library Slider's existing bounded
   behavior; do not embed a new host slider implementation in the composite.
6. Test the component: drag Slider, use keyboard arrows and verify the sample value
   changes. The label stays “Volume”; the control takes its accessible name from
   the exposed label by an explicit label association.
7. Optional enhancement: insert a value Text beside Title in a horizontal Header
   container. Bind it using an admitted number-to-text formatter. If none is
   admitted, show the sample text as unbound and defer the live readout; do not
   fabricate a general string-expression system or silently duplicate state.
8. Apply. Insert two linked instances in a scratch settings panel, named Audio
   and Display. Override labels to “Volume” and “Brightness”; bind each value to
   a distinct numeric fixture/app property. Verify changing one value does not
   change the other, while a shared style update reaches both.
9. Narrow the specimen width and increase label length. The label wraps and the
   slider still fills the available width without a new constraint rule.

**Saved effects versus preview.** The definition owns layout, forwarding and authored
defaults. Each instance owns explicit overrides. App values remain app-owned.
Dragging in Test changes fixture state only; pressing Apply does not capture 40
or the last dragged value as a new business value unless the author edits a default.

**Cancel/back/error.** Cancel Expose returns without a partial API. Invalid range,
zero step, out-of-range default or incompatible binding produces a field-specific
error. If deleting Title would break `label` or its accessible association, J12
requires explicit repair. If a formatter is unavailable, omit/defer the readout.

**Acceptance.** This is a release-blocking end-to-end journey: blank → compose →
expose → apply → instantiate twice → customize → bind → test → modify definition
→ preserve instance values → delete/repair one dependency. A static lookalike is
not completion. Child Slider events must cross exactly one declared forwarding
path; no duplicate valueChanged delivery.

## J05. Set layout without a constraint system

**Entry/preconditions.** A root, container or child with admitted layout fields is
selected. The inspector separates the container's child layout from its own sizing.

**Steps and feedback.**

1. Choose vertical or horizontal Flow. Show child order, gap, padding and the
   nine-position alignment control with a concise selected value such as Top/Left.
2. Set Width and Height independently to Fixed, Hug or Fill. Fixed reveals a value;
   Hug shows measured result read-only; Fill shows available parent allocation.
3. Advanced min/max bounds are collapsed until used. Individual padding expands
   from horizontal/vertical values with clear linked/unlinked state.
4. Packed versus Space between is explicit. Space between distributes remaining
   main-axis space; the inspector explains when there is no free space to distribute.
5. Switch clip/scroll behavior where supported. The preview outlines the viewport
   and makes overflow visible in Edit; Test uses actual clipping and hit rules.
6. Free position is an explicit layout participation choice. Its X/Y and anchor
   settings appear only when admitted. It does not appear as a disguised constraint
   editor. Rotation/flips require renderer and input support before becoming editable.

**Saved effects versus preview.** The document stores declarative sizing/flow values,
not measured preview geometry. Canvas zoom/pan and specimen width are local. The
compiler derives layout from the canonical graph.

**Cancel/back/error.** A Hug parent with an unconstrained Fill child needs a defined
policy. Explain the invalid circular sizing dependency and offer Fixed parent or
Hug child; do not silently invent dimensions. Invalid min>max, nonfinite dimensions,
negative padding or exceeded coordinates blocks Apply with the responsible field.
Wrap/grid remain visibly unavailable until their layout contracts are qualified.

**Acceptance.** Horizontal/vertical × Fixed/Hug/Fill × nine alignments have fixture
coverage. Long text, empty containers, mixed child sizes, overflow and nested
padding remain comprehensible. Layout changes preserve selected semantic IDs.

## J06. Apply a style, edit variables and inspect modes/states

**Entry/preconditions.** The selected control has a known style contract. The
Appearance section starts with the style name, owner and effective mode.

**Steps and feedback.**

1. Select a compatible named style. The preview updates its coordinated background,
   text, icon, stroke, focus and geometry fields. Incompatible styles are excluded
   with a visible explanation, not merely absent without reason.
2. Select a field's source indicator. The user can inspect the style default,
   current state patch, variable reference or instance override. A resolved hex
   value is secondary to its source, never a replacement for the binding.
3. Bind background to a descriptive variable. In the variable editor, show its
   name/owner/type and Light/Dark values together. Rename preserves identity.
4. Switch Preview mode to Light or Dark; switch state preview to default/selected/
   pressed/disabled/focus. Each is clearly local simulation, not saved app behavior.
5. Edit the variable where authorized. Show affected uses in the admitted document,
   preview each mode and surface contrast warnings at actual usages.
6. To customize only this control choose Set literal or another variable on the
   field. Reset override removes that layer and reveals the inherited source.
7. An explicit container mode appears under its presentation settings; descendants
   otherwise inherit. A dark subpanel can live within a Light root.

**Saved effects versus preview.** Style references, explicit mode settings and
variable values are draft edits. Preview mode/state is local. System host mode
is not replicated automatically. Color-only edits are eligible for retained paint
when dependency analysis proves them safe; geometry fields still require layout.

**Cancel/back/error.** Cancel a color editor restores the prior local field expression,
not a stale resolved value. Imported variables offer Fork/override rather than a
fake edit. Low contrast warns without silently choosing text colors. Unknown image
or transparency backdrops report Unknown, not Pass. Deletion of a used variable
requires replacement or removal of all references in one validated candidate.

**Acceptance.** Two LabelledSlider instances share styles; a local override stays
local; Reset resumes inheritance. Mode/state switching does not emit business
intents or resize controls. Renaming a referenced variable breaks no binding.

## J07. Expose properties, actions and slots

**Entry/preconditions.** The author is editing a writable definition, not an instance.
API exposes only supported typed targets and bounded action/slot contracts.

**Steps and feedback.**

1. API shows Properties, Actions and Slots with concise names/types and target
   paths. Empty sections explain their purpose and offer a single Add action.
2. Choose an internal property → Expose. Supply public name, label, default,
   numeric constraints where relevant, and whether instance override/app binding
   is permitted. Stable identity is distinct from editable name.
3. Choose an action/outlet → Expose and inspect payload type and forwarding path.
   Required handlers cannot be replaced with untyped string event names.
4. Add a slot by selecting its actual host container and accepted child classes,
   cardinality and layout participation. Show a specimen placeholder only as
   editor decoration, not an automatically saved child.
5. Preview the instance-facing inspector before Apply. Mark API changes as compatible
   or requiring dependent repair/new admission; show concrete affected uses.

**Saved effects versus preview.** API records and forwarding live in the definition.
Specimen controls are local authoring helpers. Changing a display label preserves
identity; changing type/meaning is a contract change, not a rename.

**Cancel/back/error.** Invalid default/bounds, multiple drivers of a noncomposable
property, cycles or unsupported action payloads block Apply. Removing an exposed
property with uses follows J14's repair principle. No silent orphaned overrides.

**Acceptance.** LabelledSlider exposes the necessary surface without exposing every
internal drawing primitive. The instance inspector names the public API; an author
can navigate from that API back to its implementation target.

## J08. Bind application data and actions

**Entry/preconditions.** An instance belongs to an app document with admitted typed
state/action descriptors. Editing a reusable library definition does not grant
access to arbitrary application state.

**Steps and feedback.**

1. In Instance → Data select an exposed property and choose Bind. The picker shows
   compatible declared app properties, their types and read/write direction.
2. Choose Volume for Audio and Brightness for Display. The binding row shows the
   complete source and a clear bound indicator; it does not look like a literal.
3. Connect valueChanged to an admitted typed app action or declared property write
   through the existing component contract. Explain one-way display versus a
   control whose event updates state; avoid a vague automatic two-way checkbox.
4. Test using copied fixture values. Show intent/result traces with property/action
   names and validation errors, without revealing private remote payloads.
5. Apply the wiring. The confirmed app continues to own its business values.

**Saved effects versus preview.** Bindings and explicit literals are authored source.
Fixture changes and Test event traces are local. Setting a style never rewrites
app values; removing a UI instance does not delete its underlying app data.

**Cancel/back/error.** Type mismatch or unavailable write direction is disabled with
a reason. A removed property/action leaves an explicit broken connection diagnostic
in the candidate and blocks its application, while the active graph stays usable.
Cancel Bind retains the old source exactly.

**Acceptance.** Distinct slider instances bind independently, defaults do not overwrite
existing app values, and each event reaches one declared endpoint. Read-only app
state cannot be made writable by changing an inspector flag.

## J09. Modify an existing component definition

**Entry/preconditions.** Open a definition from Library, or choose Edit definition
from a linked instance. Imported definitions first offer read-only source or Fork.

**Steps and feedback.**

1. The heading says “Editing definition [name]” and shows known uses and confirmed
   base revision. A breadcrumb returns to the originating instance after editing.
2. Make layout, text, style or API edits in the local working copy. The preview
   updates locally. The impact panel lists known direct and transitive uses, with
   a clear boundary around the admitted document/library scan.
3. Compare the proposed specimen with representative instances, including explicit
   overrides and long labels. Preserved overrides are visibly marked.
4. Apply shows a concise change summary and any blocking or advisory diagnostics.
   Confirmation updates known linked occurrences atomically after admission.
5. Return to the originating app/instance. Selection and business values remain;
   only inherited fields follow the definition update.

**Saved effects versus preview.** All shared changes wait for Apply. Editing a
library source does not publish a signed update to every consumer; J19 handles
that separate step. Usage counts never imply knowledge of every remote copy.

**Cancel/back/error.** Navigating away with edits offers Keep working, Discard changes
or Apply when valid. A stale generation preserves the candidate for J17. A
contract-breaking change shows affected API and dependency repair requirements.

**Acceptance.** Changing the shared gap affects both linked specimens; an overridden
label stays overridden. No definition edit resets app state or pointer identity.

## J10. Override and reset a linked instance

**Entry/preconditions.** Select the occurrence rather than its definition. The
heading says Instance of [definition] and includes Edit definition separately.

**Steps and feedback.**

1. Edit an exposed label, layout or appearance property. A local override badge
   appears beside that field, with its inherited value available on inspection.
2. The parent definition and sibling occurrences remain unchanged in preview.
3. Choose Reset on one field. Remove only the selected scope/state override. If
   another intermediate override remains, show that source instead of claiming
   the field now comes directly from the library default.
4. Reset all overrides requires a review list, especially for bindings and labels.
   It must not erase application values or structural children in slots.
5. Apply, then change the parent default and verify the reset field follows it.

**Saved effects versus preview.** Overrides are addressed by stable occurrence path.
Preview mode and Test values are not overrides. A literal matching the default
is still explicit until reset; do not infer inheritance from value equality.

**Cancel/back/error.** Invalid values retain the input draft and show an error. Reset
of an observed concurrent property uses existing causal semantics and does not
silently erase unseen edits. Unexposed internals are read-only in instance scope.

**Acceptance.** Reset restores the source relationship rather than copying today's
value. Sibling instances and underlying app data remain unchanged.

## J11. Duplicate, fork, extract and preserve reuse intentionally

**Entry/preconditions.** The selection can be an occurrence, a definition, a group
of owned children, or imported read-only source. The command names say what is copied.

**Steps and feedback.**

1. Duplicate instance creates another linked occurrence with a fresh identity,
   retaining explicit authored assignments but not current Test/runtime state.
2. Duplicate definition creates a separate editable definition, new identity and
   independent future edits. Internal links to external definitions remain links.
3. Fork imported definition copies only permitted source into the editable owner,
   records provenance and remaps owned IDs; dependencies stay explicit and pinned.
4. Extract component groups selected siblings into a new definition and replaces
   them with a linked occurrence. Present the extracted API/bindings and validate
   that ownership, layout order and event targets remain representable.
5. Confirm the planned result in the candidate before Apply. Prefer a simple
   refused extraction over inventing hidden forwarding machinery.

**Saved effects versus preview.** Accepted copies allocate new durable identities.
A duplicate name is a display concern, not identity reuse. A fork is not a magical
live link to its upstream source.

**Cancel/back/error.** Cross-boundary selection, unsupported binding remapping or
capacity overflow rejects extraction with the concrete problematic targets.
No partial copied definition remains in confirmed source after failure.

**Acceptance.** Duplicate instance follows future shared edits; duplicate definition
does not. Fork provenance is visible; extraction retains behavior or is refused.

## J12. Remove a node or control from a component

**Entry/preconditions.** A child in definition-edit scope is selected. Removing an
instance from an app is explicitly a separate scope of the same command.

**Steps and feedback.**

1. Delete opens an impact summary when the node has owned descendants, exposed API,
   connections, label associations, slot content or other references. A simple
   unreferenced leaf can remove locally with Undo available before Apply.
2. Show the owned subtree that will disappear and references that would break.
   Removing the Slider in LabelledSlider highlights value/valueChanged forwarding.
3. Offer Cancel, or an explicit repair plan: remove dependent public API, redirect
   its supported targets to another compatible child, or replace the control.
4. Preview resulting layout and selection fallback to the nearest surviving parent.
   Only Apply publishes the repaired candidate.

**Saved effects versus preview.** Removing a visual instance does not remove its
definition, shared style, dependency package or app business data. A removed
occurrence cannot be targeted by delayed operations because identity is not reused.

**Cancel/back/error.** Required root deletion is refused with “A component needs a
root; replace the root or delete the component”. Dangling required references
block Apply. Undo restores the local candidate with its bindings before Apply.

**Acceptance.** Deleting Title reports accessible-name/API effects; deleting an
unbound decoration is simple; deleting Slider never leaves invisible event wiring.

## J13. Delete an unused component definition

**Entry/preconditions.** The writable definition has zero known uses in the admitted
owner graph, no required preset/export role and no unresolved scan error.

**Steps and feedback.**

1. Choose Delete component from its persistent Library details action.
2. Show its name, owner and usage-scan boundary: “No references in this document”.
   Published historical revisions are immutable and not being erased.
3. Confirm removal into the candidate; the library selection moves to its parent.
4. Apply validates usage again against the captured/current source. The confirmed
   library no longer offers the definition for insertion.

**Saved effects versus preview.** The definition is removed only from the editable
source/revision. Existing signed packages and external copies are unchanged.
Removal of an exported published component is a contract-breaking publication.

**Cancel/back/error.** Unknown usage due to inaccessible/incomplete source is not zero.
A newly added concurrent dependency makes Apply stale/rejected; return to J14.

**Acceptance.** Unused local deletion works, cancel does nothing, and immutable
historical consumers are neither erased nor silently upgraded.

## J14. Delete a component that is in use

**Entry/preconditions.** The component is referenced by other definitions, linked
instances, presets, required exports or property/action forwarding. Dependencies
may be direct or transitive. The user has selected the component, not one occurrence.

**Steps and feedback.**

1. Delete shows “Cannot delete while in use” and a dependency list grouped by
   definition, document instance, preset/API, and signed export. Every known item
   links to its source. Distinguish direct references from transitive impact.
2. The primary safe action is Cancel/Keep component. Offer Retire from new insertion
   (J15) and Replace references when the author can edit every affected local use.
3. Replace references asks for a compatible replacement definition. Show explicit
   property/action/slot mapping and any missing required fields. Name similarity
   is not compatibility. Keep per-instance values/overrides only through mapped IDs.
4. Preview each known affected composition. Confirm one bounded repair transaction
   replacing all local references and removing the now-unused definition.
5. If the plan cannot fit one admitted atomic edit or crosses owners/permissions,
   use a staged process: retire, update each editable dependent, rescan, then delete
   when locally unused. Do not invent a cross-Space transaction.
6. Published consumers pinned to older signed revisions keep those complete packages.
   A new breaking library revision cannot masquerade as a compatible update.

**Saved effects versus preview.** Default deletion is blocked. No silent cascade,
placeholder replacement, app data deletion or permission escalation. A complete
accepted repair changes only enumerated editable sources in its authorized scope.

**Cancel/back/error.** Cancel retains the original candidate. Unmapped required API,
readonly users, incomplete scans or stale revisions block removal. Offer navigation
to the blocking use, not a generic “dependency error”. “Detach all” is deferred:
copying graphs, guest behavior and identity contracts safely is not a cheap fallback.

**Acceptance.** Attempt deleting Slider while LabelledSlider references it: refusal
names LabelledSlider. Attempt deleting LabelledSlider with two app occurrences:
refusal names both. A compatible explicit repair succeeds atomically or leaves
all confirmed uses intact. Remote unknown consumers are never reported as updated.

## J15. Retire/deprecate a component

**Entry/preconditions.** The author owns the editable definition/export metadata.
Existing instances remain valid; retirement is not deletion or package garbage collection.

**Steps and feedback.**

1. Choose Retire from new insertion and enter a concise reason, optionally linking
   to a replacement. Show how the Library tile will appear.
2. After Apply, ordinary Insert hides retired items by default; a Show retired
   filter exposes them with a warning. Existing selections and definitions remain
   navigable and editable where permitted.
3. Existing instances show a nonmodal advisory with the replacement link. An author
   can open an explicit replacement plan; there is no automatic migration.
4. Re-enable insertion when retirement was mistaken, retaining identity and uses.

**Saved effects versus preview.** Retirement metadata affects discoverability. It
must not disable runtime controls, alter app values or break exact historical pins.

**Cancel/back/error.** An invalid replacement link is reported. Lack of permission
offers read-only usage information only. No retirement flag substitutes for a
capability/security revocation mechanism.

**Acceptance.** Retired controls continue rendering and responding; new insertion
is deliberately harder without making old compositions uneditable.

## J16. Apply, undo and redo

**Entry/preconditions.** One local working copy is based on a confirmed document
revision. The top bar continuously distinguishes Clean, Unapplied and Invalid.

**Steps and feedback.**

1. Each meaningful edit creates a local undoable command. Coalesce one slider/color
   drag or one text editing session into a sensible entry, not hundreds of samples.
2. Undo/Redo before Apply changes the candidate and preview only. Selection follows
   surviving semantic identities and the inspector retains valid input drafts.
3. Apply validates all proposed graph/API/style/connection changes and shows a
   concise summary. An invalid field scrolls into view and receives focus.
4. Submission captures epoch, generation, operation identity and causal contexts.
   Show Sending, then Confirmed only after authority confirmation.
5. After confirmation the base advances and local history starts a new candidate
   boundary. Undo applied change is an explicit compensating edit against current
   source, not local time travel or a promise to restore an old shared snapshot.
6. A completed apply receipt supports retry after lost replies without duplicating
   allocations, deletion or publication.

**Saved effects versus preview.** One valid Apply commits one coherent edit batch.
Independent typed property causal conflicts still follow their existing semantics;
the UI must not silently weaken them to last-writer-wins.

**Cancel/back/error.** Discard resets the candidate to the confirmed base, preserving
local workspace preferences. Once submitted, Cancel cannot promise to retract an
accepted edit; query its receipt and show the result. Invalid/stale/rejected Apply
retains the proposed edits for correction. See J17/J20 for uncertainty.

**Acceptance.** Apply cannot create a half-saved LabelledSlider. Replay of an accepted
create has one identity allocation. Undo after a rival edit cannot erase that rival.

## J17. Reconcile stale or conflicting work

**Entry/preconditions.** Another author or signed update changes the source while
this editor has unapplied work, or a causal property has concurrent values.

**Steps and feedback.**

1. Show “Source changed” with the confirmed revision while retaining the local
   candidate. Do not silently refresh the canvas and overwrite the author's draft.
2. Offer Review changes: compare the captured base, current source and proposed
   edits using named properties and hierarchy operations, with IDs available in detail.
3. Automatically revalidate only nonconflicting operations in a proposed rebase;
   applying the rebased candidate still requires a deliberate user action.
4. For conflicting property values choose current, mine or an explicit new value.
   For deleted/moved targets, navigate to the nearest surviving scope and require
   an explicit remap or discard of that operation.
5. Confirm a new generation/causal context after review. Send one new transaction;
   do not keep retrying rejected structural edits in the background.

**Saved effects versus preview.** Review and rebase are local until Apply. A conflict
is not resolved merely by hiding a badge or picking whichever value rendered last.

**Cancel/back/error.** Keep working retains the old-base candidate but Apply stays
blocked until review. Discard proposed changes is explicit. Repeated rival changes
can stale the new submission again; retain it and explain rather than spinning.

**Acceptance.** Two editors creating/reordering/deleting against one generation
cannot both claim confirmation. A stale deletion never removes a newly added use.

## J18. Test components and representative fixtures

**Entry/preconditions.** A valid local candidate or confirmed component is selected.
Test operates an isolated specimen with declared fixture state and no production
side effects. Invalid source can remain in Edit but cannot enter Test silently.

**Steps and feedback.**

1. Select Test. Selection handles disappear; controls operate. A persistent Test
   label, fixture name and Exit Test action prevent confusion with a real app.
2. Choose fixtures: default, long/empty text, min/max values, disabled, selected,
   error/loading where supported, Light/Dark, narrow/wide parent and nested mode.
3. Interact with pointer, touch and keyboard. A compact event log shows declared
   changes and rejected actions. Reset fixture restores its deterministic initial state.
4. Return to Edit with selection restored. Fixture values remain local while the
   editor stays open; they never become component defaults through Apply.
5. Record a qualification result by control/journey ID with what was actually
   tested: prototype model, HTML UI, compiled core, browser or native Frame.

**Saved effects versus preview.** Mode/state/fixture controls create no document
writes. Saving a new fixture is an explicit authored test-data action, not assumed
by playing with a control. No real sign-in/payment/permission action runs in Test.

**Cancel/back/error.** A malformed fixture shows validation failure; the previous
valid specimen remains. A modal Test dialog traps interaction within the specimen,
not the Designer's own Exit Test action. A runaway guest is bounded and reports
its failure without destabilizing the editor.

**Acceptance.** The same control can be reviewed in Edit and actually operated in
Test. LabelledSlider valueChanged is observable exactly once per declared event;
Test does not mutate the live Audio/Display business values.

## J19. Publish a library, adopt updates and roll back

**Entry/preconditions.** Apply has produced confirmed source. The publisher identity,
permissions and existing signed-release workflow are already configured. Publishing
is unavailable from an invalid or unconfirmed working copy.

**Steps and feedback.**

1. Choose Publish from Library. Show source revision, previous signed revision,
   version/release note, exported definitions/styles/contracts and compatibility.
2. Run bounded validation and specimen qualification. Show blocking errors by
   component with a navigation action; warnings must have explicit disposition.
3. Build and sign the immutable candidate using the authorized existing publisher.
   The UI says Published only after the signed closure exists and verifies.
4. Application adoption lists available exact revisions, contract compatibility,
   affected imports and preserved overrides. “Published” does not mean every app
   has adopted the new version.
5. Adopt compatible update through the existing admission/confirmation/gesture-safe
   activation boundary. Display Waiting for interaction to finish when appropriate;
   never force through a held gesture or connected unconfirmed edit.
6. Rollback selects a verified compatible signed revision and performs the same
   complete admission/activation checks. It replaces code/design, not app data.

**Saved effects versus preview.** Source Apply, package publication and app adoption
have distinct receipts/states. Compatible graph/style/guest changes can reuse hosts
only within declared capabilities. Breaking contracts require explicit new
admission/reference repair, not a compatibility toggle in the UI.

**Cancel/back/error.** Signing/build/network failure keeps confirmed source and old
active package intact. After submission, resolve publication outcome before retry.
Readiness is not visual acceptance. Failed candidate activation leaves the current
verified library running; surface the reason and retain known rollback access.

**Acceptance.** A style-value update and rollback preserve per-instance overrides,
app values and semantic identity. Removing an exported used API cannot pass the
compatible-update path. No “success” label precedes verification/activation.

## J20. Recover from disconnect, restart or lost replies

**Entry/preconditions.** The connection drops while clean, while editing, during
Apply or during publication. Current policy allows disconnected RAM-only edits to
be discarded on upgrade; durable offline storage is not promised.

**Steps and feedback.**

1. Show Offline with last confirmed revision. Existing local work stays visible
   while the process remains alive, clearly marked Unsent or Outcome unknown.
2. Disable authority-dependent Apply/Publish; local editing and valid preview may
   continue within available admitted source. Do not show a spinning Save forever.
3. On reconnect, discover the current signed release through the stable control
   hello independently of state recovery. Negotiate compatible state before bytes.
4. Query the known operation receipt for an uncertain Apply. Accepted outcome
   advances the base once; rejected outcome retains the candidate; stale/unknown
   history needs deliberate reconciliation rather than duplicate submission.
5. If an upgrade will discard unconfirmed RAM work, explain the actual policy and
   offer continued local review while possible. Do not claim a durable recovery
   stash or file export unless implemented and qualified.
6. On a fresh process after RAM loss, open the last confirmed document and state
   that unsent session work was not saved. A missing last selection follows J01.

**Saved effects versus preview.** Confirmed document/receipts follow existing durable
server contracts. Workspace preferences and an in-memory undo stack have separate
lifetimes. A release rollback does not restore business data or undo shared edits.

**Cancel/back/error.** A state recovery error cannot prevent release discovery.
Do not request repeated login if the real failure is an unsupported codec. If
permissions changed, keep honest read-only/unsent status and stop retries.

**Acceptance.** Lost reply → reconnect produces exactly one create/delete. Offline
preview never claims durability. Restart recovers confirmed work and does not
silently turn a stale RAM operation into an edit of another document epoch.

## J21. Review accessibility and input behavior

**Entry/preconditions.** A control has a valid specimen, semantic role, accessible
name/value/state where applicable, and actual pointer/keyboard behavior.

**Steps and feedback.**

1. Accessibility inspection identifies role, label association, value range,
   enabled/selected state and focus order using descriptive names.
2. The author reviews the component without pointer hover: keyboard navigation,
   activation, value adjustment, Escape/close and focus return after a modal.
3. Test actual touch targets and scrolling. A small thumb/chevron may have a larger
   effective target, but it cannot overlap a sibling's actionable region.
4. Inspect required contrast in both modes and relevant states. Show actual
   foreground/backdrop sources. Unknown complex backdrop is not a pass.
5. Reorder focus explicitly only when semantic order cannot follow visual order;
   ordinary hierarchy changes should not require a second manually maintained list.
6. Ensure decorative selection outlines, focus rings and surface overlays pass
   input through, while modal barriers still consume outside interaction.

**Saved effects versus preview.** Semantic associations are authored data; focus,
pressed state and test keyboard navigation are local transient behavior.

**Cancel/back/error.** Missing label, invalid range or unsupported semantics produce
specific diagnostics. Do not hide failure with a generic accessibility score.
Contrast warnings cannot be fixed automatically by changing user color choices.

**Acceptance.** LabelledSlider has an accessible “Volume” name and numeric range;
Button/Toggle/Field/Segment/Dialog fixtures have keyboard and touch coverage.
Clipped visible controls remain clickable; blank modal gutters do not hit the app.

## J22. Resize the workspace and change preview dimensions

**Entry/preconditions.** A landscape Frame or desktop browser is editing a component.
The user may be halfway through a draft, color drag, hierarchy scroll or text entry.

**Steps and feedback.**

1. The same canvas/task dock adapts automatically to viewport width and available
   height. No device/profile selector is offered. Preserve the active task and
   interaction mode; tabs and Edit/Test remain visibly distinct.
2. Resizing panels/window updates editor layout only. Preview parent width/height
   is a separate labeled specimen control; authored Fixed dimensions stay authored.
3. Changing the specimen width demonstrates layout and wrapping. Fit to selection
   changes canvas zoom, not component scale or stored width.
4. Opening the on-screen keyboard keeps the active field, value and validation
   visible; Done returns the dock to its prior scroll and selection.
5. On unsupported portrait phone dimensions show an honest unsupported editor
   state; preserve the document and local work status instead of squeezing panels.

**Saved effects versus preview.** Panel sizes, active task, zoom, pan and specimen size
are local preferences/test context. No shared document revision is created.

**Cancel/back/error.** A resize during pointer capture must follow existing host
input rules and avoid retargeting the gesture. Escape cancels an editor resize,
not the app's unsent property change. Unsupported dimensions do not auto-discard.

**Acceptance.** A 1280×800 logical landscape design review and larger desktop review
can complete J04 without hidden essential actions. Phone portrait is deliberately
out of scope, not a failed responsive layout that must be patched.

## J23. Handle assets, unsupported capabilities and resource limits

**Entry/preconditions.** A requested control or property may need images, typography,
blur/effects, platform permissions or scene capacity beyond the admitted renderer.

**Steps and feedback.**

1. Library specimens declare capability requirements. A photo cropper or glass
   effect can appear in a future catalog section with “Requires image/effect
   backend”; it is not offered as a working insert when unavailable.
2. A valid insert/edit runs bounded admission. If it exceeds node/control/source
   byte/coordinate limits, show the requested amount, allowed budget and affected
   subtree where known. Never silently increase a cap or drop children.
3. Offer practical remedies: remove an unused child, simplify nested composition,
   use a scrolling viewport, or choose an admitted simpler control. Do not invent
   offscreen virtualization as a claim unless implemented and measured.
4. Asset import, when separately implemented, validates supported formats, byte
   dimensions, licensing/source and signed package inclusion before referencing it.
   Broken/missing assets are a diagnostic; there is no runtime network fallback.

**Saved effects versus preview.** Capability declarations and asset references are
part of the admitted signed source. A disabled future specimen creates no runtime
object. Resource counters describe current candidate admission, not all system RAM.

**Cancel/back/error.** A rejected operation leaves prior confirmed source and the
last valid preview intact. The author can inspect/remove the invalid local edit.
Unsupported native-only SwiftUI behavior is not reimplemented by a hidden browser hack.

**Acceptance.** Rich media/effect controls are honestly deferred where necessary;
resource failures identify a useful remedy; no mockup capability is presented as
proof of actual native/Web implementation.

## J24. Guide first use and catalog progression

**Entry/preconditions.** The author is new to Designer or validating a new control
family. The library may be empty, but existing users should not be forced through
a tutorial before editing their own component.

**Steps and feedback.**

1. The empty library offers Create blank component and Build a LabelledSlider
   guided exercise. The exercise creates an explicitly marked scratch candidate,
   not hidden permanent sample data.
2. Show one purposeful step at a time beside the real tool: insert Text, insert
   Slider, set layout, expose API, test, apply, then use two instances. The author
   can leave the guide and use the ordinary inspector at every step.
3. Each step has a visible outcome and a recovery route, following the structure
   of a complete app journey rather than a tooltip tour of unrelated features.
4. The control validation catalog groups representative families. Each control's
   card links to its specimen, relevant journey IDs and qualification status.
   A prototype pass and a real Frame pass are distinct labels.
5. Progress reflects completed checks with evidence, not just opening a specimen.
   Returning later uses stable control/journey IDs and the package recovery guide.

**Saved effects versus preview.** Guide progress and scratch fixture state are local
unless an explicit save is implemented. Apply saves only the author's selected
component work; it never publishes a signed library implicitly.

**Cancel/back/error.** Exit guide preserves the current candidate and returns to
normal editing. Reset exercise is clearly scoped to scratch data. The author can
restart a failed step without resetting the library or unrelated app state.

**Acceptance.** A new author can finish one useful composed control and understand
Definition versus Instance, Apply versus Publish, and preview versus business
state. The second control can be built without the guide.

## J25. Create and maintain insertion presets

**Entry/preconditions.** A component definition has an admitted exposed API. The
author wants named insertion defaults, such as Volume and Brightness, without
forking the definition. A preset is an insertion recipe, not another definition.

**Steps and feedback.**

1. Library → selected component → New preset. Enter a unique name and choose the
   exact owning definition. The editor lists its exposed properties and compatible
   styles; private internal fields are not preset endpoints.
2. Set only the desired assignments. Leave other fields inherited from the
   definition. Preview both modes and a sample instance without changing app data.
3. Apply creates the preset in the working document. Publish/export is separate.
4. Inserting the preset creates a new linked component instance, copying the
   preset's authored assignments once. Copy expressions/references, not today's
   resolved colors. Later shared styles/variables still resolve through their refs.
5. Editing preset defaults affects **future insertion only**. Existing instances
   keep their assignments. The editor states this before Apply; an optional
   “Update selected instances” is a separate reviewed local edit with explicit
   field mappings and override preservation, not automatic propagation.
6. Duplicate creates a fresh preset ID. Fork an imported preset into an authorized
   local owner, retaining admitted definition/style refs or explicitly remapping
   them. Retirement removes it from new insertion; unretire is explicit.
7. Delete an unexported local preset after reviewing remaining insertion/export
   references. Historical instance provenance is informational, not a live preset
   dependency. Existing instances still reference their component/style and survive.
   Public export removal takes breaking admission even if locally unused.

**Saved effects versus preview.** Saved preset assignments reference stable typed
properties. Each instance owns its copied assignments, optionally with informational
preset origin. Preset edits never rewrite existing instances or business values.

**Cancel/back/error.** Cancel/Discard leaves the old preset. Definition contract
changes require explicit missing-field/type repair. Deleted style/variable refs
cannot silently become literals. Incompatible mappings or stale Apply reject the
complete candidate, retaining the local proposal for correction.

**Acceptance.** Create → insert A → change preset → insert B proves A retains its
old assignments and B gets new defaults. Both still receive changes from shared
style references. Duplicate/fork identities differ; retire/delete leaves A/B valid.

## J26. Create and maintain styles and variables

**Entry/preconditions.** The author has write permission for a design system, or
can fork an imported source into an authorized local system. Controls declare a
compatible style contract. Color variables are the initial supported variable type.

**Steps and feedback.**

1. Library → Variables → New color variable. Enter a unique descriptive name and
   both Light/Dark values. Initializing both from one color is explicit. Preview
   resolves the pair but does not write another client's mode preference.
2. Apply creates a stable variable ID. Rename changes its display name only.
   Duplicate creates a new ID/pair; no alias chain is invented.
3. Library → Styles → New style. Select a compatible contract such as
   button.appearance.v1, name it, and populate required default fields from
   literals or typed variable refs. An incomplete required field blocks Apply.
4. Add supported color-state patches through named scopes. A state patch does not
   introduce another layout or behavior definition. Preview default, selected,
   pressed, disabled and independent focus in both modes, including contrast.
5. Apply, then choose the new style on a component. The impact view shows local
   known occurrences and surviving instance overrides. Export/publish remains
   separate from the shared document edit.
6. Duplicate/fork style creates a new style ID. By default it preserves references
   to the same admitted variables. An explicit “Copy variables too” remaps owned
   variable IDs in one candidate; it never copies only resolved Dark colors.
7. Retire a style/variable from new picks while retaining resolution of existing
   references. Unretire restores discoverability. Deleting a referenced record is
   blocked until explicit replacement repairs every local reference in the same
   batch. A required published export cannot be removed compatibly.

**Saved effects versus preview.** Stable IDs and exact owners identify records.
Field source references, state patches and mode pairs save atomically. Shared
variable changes reach their bound uses; instance literals remain local. Retirement
is discoverability metadata and does not change existing resolved values.

**Cancel/back/error.** Dirty navigation offers Apply/Discard/Stay. Unknown owner,
wrong variable type, missing mode, duplicate name, incompatible style contract,
reference budget or stale generation is a specific rejection. Read-only imports
are inspected/forked, never edited by renaming their labels.

**Acceptance.** Create pair → bind style → use twice → rename variable → update
both modes → override one occurrence → reset proves identity/inheritance. Duplicate
with shared versus copied variables has the displayed effect. Retire keeps old
uses; attempted in-use deletion blocks without dropping references or state patches.

## J27. Change visibility without deleting content

**Entry/preconditions.** Visibility capability is admitted for the selected node.
It exposes Visible, Hidden (keep space) and Collapsed (remove from flow). Before
that capability is qualified, the field is explicitly unavailable with a reason.

**Steps and feedback.**

1. Select the node in Layers. Choose Hidden; its bounds remain in layout while
   paint, hit targets and semantic focusability disappear from the Test preview.
   Layers retains a Hidden marker and the same source identity.
2. Choose Collapsed; siblings reflow as though the node had no flow footprint.
   Its source, slot content, app values, overrides and connections remain intact.
3. Choose Visible; the same IDs and retained values return. Focus is not forcibly
   stolen from the user's current valid control.
4. Apply confirms the source change. A running preview gesture on a now-hidden
   control receives Cancel before the candidate's input map activates. Focus moves
   to the nearest visible eligible owner/control, otherwise the canvas.
5. Inspect opacity separately. Zero opacity changes paint, not layout/input. Warn
   when an ordinary interactive control becomes invisible. Explicit modal input
   barriers remain barriers until the modal lifecycle deliberately dismisses them.

**Saved effects versus preview.** Visibility is a typed source property, independent
of enabled, opacity and color modes. State simulation is local; changing the saved
visibility field is a draft edit. No class/instance deletion occurs.

**Cancel/back/error.** Undo/Discard restores visibility and layout. A required slot
cannot be invalidated by pretending a collapsed child is deleted. Unsupported
visibility or inaccessible focus fallback is diagnosed before activation. Never
partially remove a modal barrier while leaving background controls active.

**Acceptance.** Hidden retains outer layout dimensions; Collapsed reflows; reveal
retains IDs and business values. Hidden controls cannot receive hits/focus. Capture
cancels once, modal focus returns correctly, and opacity-zero barrier input remains
explicit. These require real runtime fixtures; they are not implemented by the
current HTML prototype.

## 4. Cross-journey design review checklist

Before calling a new control family designed, walk its blank/create, insert,
layout, style/mode, API, data binding, Test, shared edit, override/reset, remove,
in-use delete, conflict/recovery and publication paths. Explicitly mark Not
applicable with a reason instead of skipping a step silently. Check empty, loading,
error, disabled, overflow, long text and inaccessible-source states.

A successful HTML prototype validates navigation, labels and the prototype model.
It does not establish bounded Rust admission, Wasmi behavior, native touch capture,
signed activation, actual Frame rendering or end-to-end presentation performance.
Those need their own evidence rows and must remain pending until run.
