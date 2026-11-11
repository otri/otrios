<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer: workspace and inspector specification

Date: 2026-11-11. Proposed UX. Narrative cases are in [J01–J24](UX_JOURNEYS.md).
The [prototype](prototype/index.html) is a deliberately smaller interaction study.

## S01. Information architecture

The top-level hierarchy is Space → editable document/library → component definition
→ nested edit scope. Show a readable breadcrumb and exact source identity on demand.
Do not put package hashes in ordinary navigation labels. Read-only imported sources
show their library/revision and a deliberate Fork action. No hidden source switch.

Library is the collection of reusable definitions, styles and variables. Layers is
the selected definition's hierarchy. Properties is contextual to the selected root,
child, linked instance or API endpoint. A source badge distinguishes Definition
from Instance editing at all times. Library selection is not canvas selection.

Top bar: current component and one plain-language Apply status, Edit/Test, Undo/Redo, Apply.
New component is a Library action; Publish is in the library menu, distinct from
Apply. Destructive definition actions live in its labeled lifecycle menu. A footer
contains the latest result, conflicts/errors and a way to reopen details. Notices
remain until superseded or acknowledged; never rely on a fleeting toast for failure.

## S02. One responsive workspace

There is no Frame/Desktop selector or saved device profile. The same canvas and
right-hand task dock adapt to available viewport width. Library, Layers and Inspect
select the contents of that dock; switching tasks does not move the canvas or
change interaction mode. Keep the current task, selection, drafts, sample values
and each panel's scroll anchor when resizing. Native Frame and browser use this
same information architecture, with their input adapters and available space.

| Control family | Treatment | Meaning |
| --- | --- | --- |
| Library / Layers / Inspect | Underlined tabs, selected indicator and tab semantics | Navigate the task dock |
| Edit / Test | One bordered segmented group with a filled selection | Change how canvas input behaves |
| Preview appearance | Labeled segmented switch, Dark/Light | Change local preview context |
| New / Undo / Redo / Discard / Publish | Command buttons in the action bar | Perform an operation |
| Apply | Primary command when a valid draft can be applied | Confirm shared source edits |
| Apply status | One plain-language status near commands | All changes applied / Unapplied changes / review or validation required |
| Revision numbers | Collapsed technical details only | Debugging; never ordinary editing feedback |

Labels Interaction and Preview appearance make independent modes explicit. The
navigation tab group keeps its accessible Workspace name without a redundant
visible Workspace heading. Tabs use Left/Right/Home/End plus normal keyboard focus. Edit/Test uses
pressed state within its own labeled group, not tab styling. Navigation never
silently switches Edit/Test, and appearance never writes an app theme preference.

At wide sizes, grow the canvas and task dock up to its readable width; at smaller
landscape sizes, retain a useful canvas and scrollable dock instead of shrinking
labels. The prototype dock ranges from 280 to 360px; production panel widths and
48-unit targets still need real Frame/scene-budget qualification. Toolbar groups
wrap as units and status can occupy another line. No essential action depends on
hover, right-click or precision dragging. Device identity is never a layout input.

1280×800 is the required Frame target; 1920×1080 is the larger desktop design
reference. Smaller landscape acceptance remains proposed. Below the supported
authoring envelope, show an honest notice and preserve work; phone portrait is
unsupported. In production, available height determines the workspace after chrome
and keyboard reservation; panels scroll internally and preserve their anchors.
The standalone conversation prototype computes available work height from the
viewport minus measured chrome, bounded to 240–720px for inline rendering. It
uses integer rounding and observes chrome/viewport changes to avoid height loops;
the upper bound is a study limit, not a device mode.

Preview parent dimensions are a separate fixture context. Changing editor width
can reflow the dock/chrome, but it must not silently rewrite authored Fixed W/H,
app values, component IDs or a chosen preview fixture size. The software keyboard
keeps the active field and validation visible, then restores its scroll anchor.

## S03. Library and blank states

Library tabs: Components, Styles, Variables. Search filters within the chosen tab;
filters never change the document. Components have name, kind/source and Active or
Retired state. Retired items appear when requested, not in normal insertion.
Read-only items can be inspected; insertion uses a verified admitted reference.
An unavailable capability gets a reason and requirements, not a pretend working tile.

Distinguish loading, no matching search, no components, no permission, offline cached
view, and admission failure. Only the genuinely empty editable library offers
“Create your first component”. Do not turn loading into a false blank document.

Create component opens name and type (2D visual component initially). The result is
a valid empty container root, Vertical, Fixed width 320, Hug height/minimum 48,
padding 16, gap 8. Canvas drop-zone text is editor chrome. Empty authored content
is legal; focus moves to Insert and the first task is visible. No sample button
is silently authored. Apply can save a blank definition; export of a blank control
requires deliberate review and no invented interaction contract.

## S04. Selection and direct manipulation

Single selection first. Tap/click a visible child in Edit selects its occurrence.
Linked instances show their owning definition and override scope; enter definition
editing through an explicit command/breadcrumb. Escape/Done goes up one scope.
Layers gives a precise alternative when a tiny/clipped/overlapping child is hard
to select. A “select parent” action is always available. Selection decoration has
input pass-through and must not turn a selected child into an input barrier.

Drag handles reorder in-flow siblings and show the insertion position; dragging
ordinary content does not accidentally switch to Free placement. Numeric Move
before/after/up/down actions remain available. Reparent only to a valid container
or compatible slot. Preview resulting inheritance/layout, then confirm in draft.
Rejected drops explain required slot/type/cycle/budget constraints and restore
original order. Moving into an imported instance's private hierarchy is forbidden.

Test routes real gestures into the isolated sample adapter. Edit/Test is visibly
persistent and switching out of Test cancels its current gesture safely. Do not
send a selection tap to business behavior or carry a Test sample into authored
defaults. “Use sample as default” would be a separate explicit future command.

## S05. Inspector structure

Only applicable sections appear. Each field shows value, units, source and Reset
when overridden. Disabled controls include a nearby reason. Pending invalid text
stays visible as a draft; never convert a half-typed number into zero.

| Section | Contents and rules |
| --- | --- |
| Identity | Name, kind, owner, stable ID in details; rename keeps references |
| Content | Text/boolean/value options supported by selected node |
| Position | In flow or Free; x/y enabled only for Free; rotation gated with hit testing |
| Layout item | Width/height Fixed/Hug/Fill, fixed lengths, per-axis min/max |
| Auto Layout | Horizontal/Vertical, alignment, packed/space-between, gap, padding |
| Overflow | Clip, supported scroll axis, definite viewport requirement |
| Visibility | Visible/Hidden/Collapsed, independent of enabled and opacity; explicit capability gate |
| Style | Compatible named style picker; source and contract |
| Appearance | Background/text/icon/stroke, opacity, radius; precise per-field source |
| Variables | Bind compatible color or literal, owner/name/mode pair editor |
| States | Preview default/selected/hover/pressed/disabled/focus without business effects |
| Data | Typed literal/forward/app binding; source identity and validation |
| API | Exposed properties/actions/slots, defaults/types and forwarding targets |
| Dependencies | Local uses, imports, retirement and replacement impact |
| Diagnostics | Resource, layout cycle, reference, contrast and unsupported capability details |

All Frame fields require direct entry and the existing software keyboard. Enter/Done
accepts a field draft, Escape/Cancel restores its previous value; neither implies
shared Apply. Tab follows visual field order on desktop. Numeric scrub/drag is an
optional accelerator with keyboard entry, not the sole editing method.

Long inspectors scroll with sticky section identity where useful. Avoid Previous/
Next pages. Virtualize only to fit current admitted scene/control/layout limits;
retained scroll/selection anchors must survive virtualized row changes.

## S06. Layout control details

Sizing modes are per axis, never a combined mystery “fit” switch. Fixed reveals an independent
length input on **both** axes: Fixed W and Fixed H. Hug/Fill show measured output
read-only; retained Fixed lengths are disabled outside Fixed mode and restored
when returning. Filling height requires a definite parent viewport just as filling
width requires a definite parent width. There is no hard-coded Fill or Fixed H. Min/max is an optional bounds
disclosure. Parent padding offers linked X/Y initially, with independent edges as
an explicit toggle. Unlinking copies the current values once; relinking requires
choosing the common value rather than silently dropping asymmetry.

Nine-point alignment is a convenient physical-axis picker for packed groups.
Space-between changes main-axis distribution, leaving cross-axis alignment; show
that reduced set instead of nine choices with misleading differences. A Hug axis
has no spare space, so alignment can be valid but have no visible effect; explain
this next to measured size. Stretch is the explicit child Fill choice, not a
constraint or an override of a child's Fixed size.

A Hug parent/Fill child cycle is rejected with the exact dependency path and two
fix actions. Unlike Figma's automatic conversion to Fixed, OIB proposes a visible
choice before changing a sibling/parent setting. Root Fill is legal only in an
explicit finite design viewport; reusable-instance dimensions come from its parent.
Text measurement uses definite wrap width. Overflow reports are visible; never
silently shrink text or ignore minimums. Show disabled Wrap/Grid capability choices
only with a clear later-stage explanation. Grid guides and actual layout are distinct.

## S07. Style and variable editing

A component selects one compatible style, for example button.primary. Its style
fields reference named variables such as action_background and action_text.
Background/text/stroke are roles within that style, not global magic variable names.
The variable editor displays both Light/Dark values next to each other so a shared
edit is reviewed as a pair. A selected mode controls preview, not which pair is saved.

Source trail example: Instance A → local override, or Instance B → button.primary
→ text → action_text → Dark. Reset removes the override. Editing a shared variable
shows uses within the admitted document; it cannot promise global counts. Imported
sources expose Inspect and Fork/override, not unauthorized shared editing.

Changing a style previews all affected occurrences with surviving overrides marked.
It does not erase app bindings. Incompatible contracts require field mapping or
cancel. Color values and opacity produce contrast diagnostics at their actual
label/backdrop uses, including overrides; missing backdrop knowledge is Unknown.
Manual override is allowed with a warning, never a silent algorithmic text-color swap.

State preview controls are separate from variant/preset selection. A disabled
fixture changes preview interaction without disabling editor controls. Editing a
state patch clearly names its scope. All-state overrides are explicit and warn
when they flatten feedback. Follow RFC 0037's fixed precedence; do not invent CSS
specificity or hidden fallback rules in the inspector.

## S08. Dependencies and destructive review

“Remove instance”, “Remove node” and “Delete definition” are different verbs. A
confirmation identifies exactly the affected scope. Removing a child lists routes,
API forwards, owned content and overrides that would become dangling; the author
repairs or explicitly removes them in the same candidate. No automatic cascade.

Definition impact panel: local direct uses, transitive dependents, presets, slot
defaults, routes and published export status. Count scope/revision is visible.
If in use, Delete is blocked and offers Inspect uses, Retire, or Replace references.
Retire preserves all existing uses. Replacement previews explicit property/action/
slot mappings, names unmapped values, and validates the full local candidate before
Apply. Externally published immutable revisions are not rewritten or removed.

Delete unused stages a candidate. Apply is still the shared confirmation boundary;
a modal confirmation does not bypass it. Undo/Discard can recover before Apply.
After Apply, conditional compensating edits must respect intervening changes.
Closing an impact review leaves the original graph unchanged. Focus returns to the
originating lifecycle action; deleting the selected item selects its surviving owner.

## S09. Failure and recovery presentation

| Problem | Visible behavior | Recovery |
| --- | --- | --- |
| Invalid field | Preserve raw draft, inline reason | Correct or cancel field; Apply disabled |
| Stale generation | Preserve working copy and diff | Compare confirmed changes, deliberate rebase/retry |
| Unknown Apply outcome | Pending status, prevent duplicate new command | Recover same receipt, then show confirmed/rejected |
| Disconnect | Cached source plus disconnected label | Local preview; no claim of shared save |
| Lost draft on process close | State RAM-only limitation before close | Discard explicitly; no promised durable recovery |
| Deleted selection remotely | Select surviving parent and explain | Reopen library; never reassign old ID |
| Unsupported primitive/font | Keep admitted active revision | Choose supported capability or qualified new release |
| Budget exceeded | Name limit and expensive subtree | Reduce preview/window/content; do not raise limits silently |
| Library admission failed | Keep active library | Inspect diagnostics, repair candidate or use compatible revision |

Dirty navigation offers Apply, Discard or Stay. Apply itself can fail; navigation
occurs only after confirmation. Publish/export/adoption gets a separate outcome and
never masquerades as a draft save. Unconfirmed RAM edits may be lost at upgrade
under current PoC policy; do not promise offline durable edits or migration.

## S10. First-run guidance and completion criteria

Teach a complete small task: blank → Text → Slider → Vertical layout → expose value
→ isolated Test → Apply → two instances → edit definition → override one → Reset.
Only then introduce dependency review and publication. Short next-action prompts
are contextual and dismissible. Do not add sign-in/paywall steps simply because a
reference flow includes them; use only the existing OTriOS authorization model.

A complete task ends with the intended confirmed result, recognizable success,
and an available next action. A visually attractive screen without cancellation,
empty state, error handling, input semantics and update behavior is not qualified.

## S11. Coverage rule for future inspector changes

Every exposed concept needs a field → source address → validation → preview →
Apply → reload/reset → test path. Cross-check width/height, min/max, leading/trailing
edges, Light/Dark, definition/instance, default/state and enabled/disabled as pairs.
A disabled capability with an explanation is preferable to a working-looking
control with a hidden constant. Read [COMPLETENESS_AUDIT.md](COMPLETENESS_AUDIT.md)
for the first audit and explicit prototype exclusions.

## S12. Useful Apply status, separate from technical diagnostics

One status surface near the command bar answers whether edits need attention:
All changes applied, Unapplied changes, Fix invalid values before applying,
Changes need review, or Pending deletion with its next action. Production also
needs Applying, Disconnected and Outcome unknown when those states actually occur.
Never report Saved while a receipt is unknown or merely after a local preview.

Generation/base counters and internal model terminology belong in collapsed
technical details. Remove the duplicate canvas status. Appearance and interaction
are both two-option segmented controls using the same component; their labels
identify independent local contexts. Neither affects Apply status unless an actual
authored field changes. The prototype's RAM-only/publication limits and simulation
commands stay in explicitly labeled Prototype & technical details.

## S13. Copy and progressive guidance

Follow [COPY_AUDIT.md](COPY_AUDIT.md). Keep labels, values, units, source/scope and
actionable errors. Move one-time guidance and prototype diagnostics into collapsed
Help/details. Remove repeated type badges, no-selection prompts and empty-result
filler. Canvas content is authored content and must not be removed as chrome.
Routine success does not require another footer message when the scene and Apply
status already communicate it. Keep rare publication outcomes and destructive
consequences explicit. Do not hide existing component API just because the creation
form is contextual to a selected node. Accessible names remain mandatory.
