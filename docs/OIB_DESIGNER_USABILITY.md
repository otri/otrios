<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer usability proposal

Date: 2026-11-04. Status: **Frame Focus implemented and live-qualified; Desktop Studio deferred**.

The [Frame Focus milestone](OIB_FRAME_FOCUS_D1.md) and
[shared-style qualification](OIB_FRAME_FOCUS_STYLE_FOLLOWUP.md) record delivered
behavior, signed Frame/Web acceptance and measured scrolling. This document
preserves the original findings, baseline and design targets; its Desktop Studio
presentation and later authoring features remain proposals.

This follow-up to [RFC 0035](rfcs/0035-oib-component-designer.md) addresses the
user's four findings: unreadable tooling text, placeholder property editing,
unusable Frame scrolling, and an overexpanded, confusing hierarchy. It preserves
D1's signed graph, stable identities, causal registers and existing application
bindings. It does not authorize D2 structure editing. The original design pass
made no runtime or live release changes; the linked milestone records subsequent
implementation and acceptance.

## Findings from the initial D1 implementation

- `Builder::label` uses text scale 1. The font contract defines this as an
  eight-logical-unit capital height, not a 16px UI font. A taller row alone will
  not make its text readable.
- `Designer::new` initializes an empty collapsed set, exposing everything.
- Hierarchy labels contain spaces for depth and also receive positioned offsets
  beside disclosure buttons. Leaves and containers do not use one column system.
- Occurrence rows use definition-node names, losing Audio/Display instance names.
  References show a wrapper and referenced root as separate navigation steps.
- `object_rows` mixes document ownership, visual children, nested references and
  appended definitions. Root-level visual attachment does not define its displayed
  parent. A UI tree cannot be recovered reliably from depth alone.
- Inspector rows repeat IDs, origins, inherited values and revision diagnostics,
  then allocate one button for every suggested value. That is diagnostic output,
  not a property form.
- Every presented scroll scene is reconstructed from a layout snapshot. Tooling
  rows are culled in 152-unit slabs; crossing a slab changes topology and can
  rebuild the shared paint cache. These are profiling candidates, not a proven
  attribution of all Frame cost.

The prior D1 qualification established boundedness and input/state correctness;
it did not establish acceptable Frame scrolling performance.

## A concrete Frame baseline

One guarded, calibrated software hierarchy pan ran against signed release
`f62027700e7d8a05a6791866231d22a7ea230f3324079e6e853a250f0dfee8ef`.
During 3.001 seconds with 30 offered input samples/second, the observer saw four
presented-readiness reports, about 1.33 reports/second. The largest observed
report gap was 1283ms. The shared OIB generation, native process and mount epoch
were unchanged. No app values were edited.

The [bounded baseline record](experiments/data/oib-designer-usability-2026-11-04/frame-baseline.json)
contains the observed counts. This is a single readiness-file observation, which
can miss presentations. It
is not independent scanout FPS, a GPU profile or a statistically useful p95.
It corroborates the responsiveness complaint and supplies a repeatable starting
gesture. Stage timing and real presentation telemetry are required next.

## Design direction

Borrow Figma's selection-driven property panel and consolidated component
properties, rather than its density wholesale. See the primary references:
[component properties](https://help.figma.com/hc/en-us/articles/5579474826519-Explore-component-properties)
and [properties sidebar](https://help.figma.com/hc/en-us/articles/360039832014-Design-prototype-and-explore-layer-properties-in-the-right-sidebar).
Collapsed components below are an OIB proposal, not a claim about Figma defaults.

Two presentations share one interaction model:

| Presentation | Composition |
| --- | --- |
| Desktop Studio | Persistent hierarchy, canvas, properties; resizable sidebars. Start near 280 / remaining / 336 logical units at 1280×800. |
| Frame Focus | Canvas plus a 352-unit properties dock. Layers opens a roughly 320-unit drawer over the canvas; it never shrinks text to fit. |

Use one compact document/mode bar, then panel headers. Remove the large repeated
page heading and move transport/generation information into a compact status
area. The canvas zoom changes application preview geometry only; tooling type
and touch targets retain their size. Frame's profile is device-local, with an
explicit local preference; screen width alone cannot distinguish Frame from a
desktop window of the same dimensions.

### Readability

| Element | Desktop starting point | Frame starting point |
| --- | --- | --- |
| Essential labels and values | At least 16 screen-pixel nominal text | 18–20 screen-pixel nominal text |
| Secondary source/status | At least 13px | At least 15px |
| Hierarchy rows | 44 logical units | 52 logical units |
| Essential touch hit zones | At least 44×44 | At least 48×48 |
| Indentation | 16 units per visible level | 20 units per visible level |

These are design targets, not values to copy into `Primitive::Text.scale`.
Calibrate nominal size, capital height and baseline with the actual font metrics
at 100% display scale on both hosts. The current integer-scale primitive may use
scale 2 as a first readable baseline. Do not implement rich typography to solve
this: tooling needs a few uniform sizes and consistent baseline alignment.

## Hierarchy: a composition tree with explicit component boundaries

The default view is:

```text
SettingsDocument
  Canvas
    ▾ Audio                         ◇ SettingsPanel instance
      ▸ Level control               ◇ LabeledSlider instance
      ▸ Power control               ◇ LabeledToggle instance
        Toggle power                Button
    ▸ Display                       ◇ SettingsPanel instance
  ▸ Objects
```

Expanding Objects reveals Configuration, an OIBObject. Definitions live in a
separate Components tab, not after hundreds of expanded occurrences. The explicit document and presented root remain selectable. Objects
is a local navigation group for nonvisual objects; it does not change ownership.

Rules:

1. Use explicit typed parent keys in the local tree projection. Composition
   follows admitted visual containment/attachments; Objects follows nonvisual
   ownership. Definitions use their own tree. Depth is derived from parents.
2. A component occurrence occupies one visible row. Its reference wrapper and
   referenced root are fused for navigation only. Selecting Level control maps to
   the real addressed root, e.g. instance 20 / ancestry [7] / node 1. An advanced
   Inspect reference action can expose the wrapper without another permanent row.
3. Label allocated roots Audio and Display, using instance names. Show component
   type as secondary context. Names never become keys or alter authored strings.
4. On first open, expand the document/canvas and the selected path only. Other
   component boundaries start collapsed; primitive internals stay closed.
5. Maintain fixed columns: disclosure hit zone, type icon, name, optional status.
   Every row uses the same depth offset, including leaves. Do not indent with
   whitespace. Long names elide with their full accessible name available.
6. Chevron toggles disclosure without selecting. Row click selects. Desktop
   arrows navigate/collapse/expand; Enter selects. Touch movement beyond the
   existing pan threshold scrolls and cancels row activation.
7. Canvas single-click selects the component boundary. A second deliberate
   enter/double-click drills into internals. A breadcrumb permits going up on
   Frame without relying on double-click. Do not automatically expand unrelated
   branches when a selected descendant is revealed.
8. Collapse state, scroll, selection and density remain local. Structural
   drag/drop, renaming authored nodes, add/remove/reparent and mutable connections
   remain outside this proposal. Preserve existing lifecycle entry points.

## Properties: real fields with source visible when relevant

The inspector starts with `Audio / Level control`, its component type, and its
editing scope. Exposed component properties come first; supported layout and
appearance follow; Connections and Advanced are collapsed. Only supported,
admitted lowerings appear editable. Do not advertise border, layout or typography
editors merely because another app renderer supports them.

| Property | Main control | Commit and validation |
| --- | --- | --- |
| Plain text | Single-line text field | Enter or valid blur; UTF-8/length constraints from admission; Escape cancels. |
| Number/dimension | Direct number field, unit suffix and stepper | Finite validated value; arrows/scrubbing use admitted nudge step. Step is not quantization unless explicitly constrained. |
| Color | Swatch, editable hex and opacity; picker on activation | Shared core picker for Frame/Web; validate RGBA8; release/Apply commits one value. |
| Boolean design property | Switch with visible label | One confirmed Boolean change; app-bound checked values remain connections. |
| Enum/policy | Labeled select or short segmented control | Only admitted alternatives. |
| App binding | Read-only field/source chip | Show `Value → volume` or `Checked → audio-enabled`; no literal-edit affordance. |

Presets may be optional palette suggestions inside a picker. They are not the
primary editor and never replace direct entry. Existing descriptor kinds named
`*Suggestions` do not automatically promise arbitrary editing: add explicit
editor admission and keep the same value constraints. Unsupported hosts reject
new required features. Coordinate the declared state protocol if wire descriptors
change, retaining release-control v1; local layout changes need no protocol bump.

Each property gets one quiet origin indicator: inherited, authored, bound,
override or conflict. A visible per-field reset appears for overrides. Details
reveal the exact source chain and address. Ordinary editing does not show numeric
IDs, repeated inherited-value sentences or active/shown generation counters.

Examples that must remain correct:

- Audio's nested label starts at authored Volume. Enter Sound and commit: only
  Audio changes. Reset clears the live override and returns to Volume, not Level.
- An explicit Volume assignment is still an override, even if the displayed
  value equals the inherited/authored source.
- Accent inherited through SettingsPanel offers Go to shared source. That opens
  SettingsPanel's actual forwarded source and states that it affects two panels.
  Editing the slider definition's masked accent must not falsely promise to
  change these occurrences. Inspecting that definition explains the masking.
- A local nested accent override masks the panel source. Reset reveals its current
  resolved value, including a shared edit received while the override existed.
- Reset and conflict resolution are separate actions. A conflict keeps the last
  confirmed canvas, shows the alternatives and permits an explicit choice. Do not
  silently resolve it by writing a signed default.

Nested exposed controls can be consolidated in an instance inspector only when
signed metadata declares that exposure; do not recursively dump every descendant
property into the parent. The prototype selects the nested Level control directly.

### Editing lifecycle and input

Raw typing is bounded device-local editor state keyed by full property address,
not a second persistent property graph. No CRDT write per keystroke. Valid
Enter/blur commits once; invalid input remains marked and never publishes.
Changing selection must never apply a draft to the new object. Retain a bounded
unapplied draft against its original address or explicitly discard it with a
visible notice; no silent retargeting. Escape discards the current raw edit.

Numeric/color gestures may preview locally, then commit a single validated value
on release. That preview must be disposable and budgeted, with cancellation
restoring the confirmed graph. If a cheap local canvas preview is unavailable,
preview the field/picker and commit on release instead of recompiling on every
move. Incoming shared changes cannot overwrite the active input buffer. They
update its source indicator, and normal causal conflict/reset rules apply.

Reuse the core keyboard/input infrastructure with a tooling field adapter.
Desktop supports caret editing, selection, Tab and Escape. Frame uses the shared
text or numeric keyboard, reveals the active field and its error/commit actions
above the keyboard, and never compresses the form to unreadable sizes. This is
tooling input support, not D3 mixed editable/read-only app behavior composition.

## Scrolling and rendering: an acceptance requirement

Profile input queue/dispatch, scene/presentation construction, glyph/paint cache,
CPU geometry, GPU upload/draw and presentation separately on the real Frame.
Reproduce pans within and across slab boundaries. Inspect, rather than assume,
which stage creates the observed stalls.

Proposed implementation direction:

- Reuse content/glyph/paint resources per pane. A scroll sample changes offset
  and clip; it does not regenerate inspector/hierarchy strings, compile OIB,
  validate the whole graph or repaint the unchanged application canvas.
- Maintain a visible row window with bounded overscan and incremental entering
  rows. Bound scene/control/cell/layout limits independently from retained cache
  limits. Collapsing content improves navigation but must not conceal a renderer
  that becomes unusable when a valid tree is expanded.
- Give stable typed row/action identity to retained content. Offscreen and retired
  actions remain rejected. Reusing a cache slot must not redirect a stale action
  or captured input to another object.
- Coalesce pending pointer moves while retaining Down/Up/Cancel ordering and
  source timestamps. Keep pan scheduling independent from authority confirmation.
- Hide verbose provenance and diagnostics until requested; do not rebuild them
  during a gesture. Keep pending publication deferred while an app capture owns
  its confirmed scene, preserving the existing input guarantees.

Before calling scrolling usable, run warmed 30-second pans on Frame at 30 offered
samples/second, separately in hierarchy, inspector and overflowing preview,
including repeated window crossings and the maximum admitted six-root fixture.
Use actual presentation telemetry: target at least 28 presented updates/second,
p95 frame intervals ≤50ms and no gesture stalls over 100ms. Treat 60fps as a later
target, not an achieved claim. Record cold first-use separately. Retain existing
512-node/128-control, coordinate, cell and layout-byte ceilings unless separately
reviewed. Pans must emit no shared edits or changes to the app values/generation.

## Proposed next milestone and ownership

**D1 usability milestone:** an author can select Audio, enter a nested label and
numeric width, pick a color, inspect its real source and reset an override, on
both hosts, without unreadable text, a flattened internals tree or scrolling stalls.

| Work package | Owner boundary | Gate |
| --- | --- | --- |
| Baseline and retained-pane scrolling | Core presentation/performance owner; coordinate `client.rs` sections centrally | Real Frame stage timings and presentation thresholds above; no state/protocol changes for scrolling. |
| Readable shell and tree projection | Core designer tree owner, plus sample shell layout owner | Actual font-size calibration, instance names, explicit parents, default collapse, reference fusion and keyboard/touch navigation. |
| Local property editors | Core tooling-input owner; narrow descriptor review with contracts owner | Plain text/number/color entry; validation, cancellation, source-aware reset, remote updates and held-input safety. |
| Integration and acceptance | Parent integrator | Shared semantics on Frame/Web, max budgets, focused regressions, independent review, signed live acceptance and compatible code rollback. |

Freeze this interaction model before assigning implementation. Partition
`client.rs` and `oib_designer.rs` explicitly; avoid overlapping edits. Keep the
performance and typography/tree work first so practical editors are built on a
responsive, readable shell. Validate each vertical slice live; do not replace
these gates with repeated broad test campaigns.

The companion [interactive design study](designs/oib-designer-usability.html)
compares Desktop Studio and Frame Focus. It is local, illustrative HTML, not the
shared Rust renderer or a shipped runtime feature. Its property edits, collapse,
selection, reset, source navigation and mode changes do not contact live services.
