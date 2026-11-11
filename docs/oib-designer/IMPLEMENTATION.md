<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer implementation goal and work record

Date: 2026-11-11. Status: **implementation in progress** on
`feat/oib-designer-interface`, based on the reviewed design/prototype history.
The active goal is to implement the final responsive Designer, qualify its
representative controls and eventually complete signed Frame/Web acceptance.

## User steering and scope

Build toward the final model in TECHNICAL_DESIGN.md and J01–J27. Current PoC data
is disposable: use a clean versioned cutover when the model changes. Do not build
migrations, preserve obsolete editor profiles or add fallback authoring engines.
The user prioritized the final design and then explicitly selected live-first
iteration so attached browsers observe the tests on the Frame. Publish coherent
slices through the existing deployment path; no extra signature-check campaign or
rollback preparation is requested. No live data inventory/backup work. Existing
normal package admission remains part of the standard publish path.

Runtime changes/tests remain LGPL in core; public types/codecs/tooling/docs remain
Apache. Keep pinned Rust/dependencies and existing scene/input/resource ceilings.
No new service, trust root, native module backend or general transaction system.

## Restart checklist

1. Read this file and current git status/diff. Do not assume a partial slice is live.
2. Read TECHNICAL_DESIGN.md, SHELL_UX.md, COPY_AUDIT.md and CONTROL_MATRIX.md.
3. Preserve the final UI decisions: one responsive canvas/task dock; underlined
   Library/Layers/Inspect tabs; matching Edit/Test and Dark/Light segmented controls;
   one useful Apply status; compact alignment/spacing; no permanent tutorial copy.
4. Consult the implementation ledger below, then focused test evidence. Prototype
   assertions are not runtime qualification. Keep incomplete capabilities gated.
5. Continue the next unfinished slice without reopening the discarded data/profile
   compatibility question. Update this record after meaningful integration/tests.

## Concrete delivery sequence

| Slice | Final behavior | Owned paths / validation |
| --- | --- | --- |
| B1 Responsive shared-runtime shell | One canvas and right task dock; local tabs/mode/context, no host-selected Studio/Frame profile; keyboard/scroll and hidden-hit correctness | core client/oib_workspace.rs, oib_designer.rs and shared client integration; real Scene/input tests |
| B2 Test isolation | Separate local sample adapter; live adapter never swapped or used for sample writes; reset on changed source, no navigation/business effects | core client/state preview fork and selectors/dispatch; outbound isolation tests |
| B3 Local editor appearance/copy | Monochrome Dark/Light chrome, source-aware fields and useful status; authored swatch colors untouched | Designer Builder/editing helpers; palette and identity tests |
| B4 Canonical layout model | Versioned per-node container/item Horizontal/Vertical, Fixed/Hug/Fill both axes, bounds, padding, packing/alignment, clipping/Free placement | public graph/source codec, core measure/compile and actual inspector fields; golden cycles/overflow/hits |
| B5 Coherent editing lifecycle | Bounded local working copy, one expected-generation Apply, typed API/slots, conditional history, dependency-safe removal/retirement/repair | extend existing canonical EditBatch and causal property transaction boundary; process receipt/conflict tests |
| B6 Styles/modes | RFC0037 typed variables/styles, state recipes, inherited modes, field-source tracing and paint proof | public schema/admission + core resolver; both-mode/source/contrast tests |
| B7 Representative catalog | Q01–Q16 and F01–F06 exercised end-to-end through the real Designer | per-row evidence in CONTROL_MATRIX; no thumbnail-only completion |
| B8 Live qualification | Coherent source/schema cutover, focused native/browser visual/input/performance | existing publish path, live slices visible to attached browsers |

B1–B3 can compile before the layout schema changes, but do not expose invented
layout controls or claim the final Designer is complete. B4–B6 can introduce a
breaking PoC state version with fresh source/data. Each slice must leave a
reviewable tested source, not a partial multi-write approximation of Apply.

## Inventory findings that constrain implementation

- Current runtime Test forwards into real app state. The final Test promise needs
  B2; relabeling existing dispatch as sample-only would be incorrect.
- Existing structural inspector already supports Name, X/Y, Width/Height, Visible,
  Text, Color and Radius through typed generation-checked edits. Reuse that
  machinery while replacing its presentation and extending the canonical model.
- Current `Visual` has frame expressions/ResizeRule, not reusable Auto Layout.
  Root composition helpers are useful for chrome, not a substitute layout ABI.
- Causal property writes and structural EditBatch currently have different commit
  boundaries. Final Apply must group them in one bounded document transaction.
- Existing host palettes are compiled constants. Local chrome appearance can be
  explicit now; component Light/Dark requires actual style resolution. Never
  recolor arbitrary authored colors by equality with a theme constant.
- Existing 512-node/128-control, source/coordinate and keyboard budgets still apply
  jointly. Hide inactive task contents from paint, input and semantics together.

## Working roles and current ledger

Parent owns technical plan, Designer state/actions/Builder/inspector integration,
final cross-contract choices and validation. The geometry role owns a new
`oib_workspace.rs`; the sample-adapter role owns client.rs and its narrowly scoped
state clone API. They share the working tree with disjoint file ownership.

Current: B1–B3 are live (see acceptance below); B4 initial runtime/UI implementation
is validated locally and preparing for live publication. Retired the host profile selector, divider
resize state and old projection module. Test uses a transportless sample fork and
waits for captured input before adopting changed source. One shared status waits
for confirmation; the inspector no longer repeats a Saved row.

Evidence before first live publication:

- `cargo test -p otrios-core --features canvas --lib oib_designer --locked --offline`: **59 passed**, one existing ignored timing diagnostic. Covers editing/deletion/conflicts, responsive task routing, hidden hits, keyboard, bounded scene construction and invalid-focus/mode transitions.
- Same command filtered to `oib_workspace`: **3 passed**, including active dock controls and unchanged authored preview paint across widths 800–1920.
- Same command filtered to `light_surface`: **1 passed**, including themed outline/input pass-through after resize/pressed fill and rejection without mutation.
- Core `cargo check`, dashboard `oib_instance_probe` check and core library Clippy with `-D warnings` passed during integration. Final component regression `--lib component_behavior_tests` passed **14/14**, including all five sample-isolation/source/reconnect cases. Live acceptance for B1–B3 is recorded below.

The HTML study under prototype/ is design evidence only (latest V09), not runtime
acceptance. No full repository preflight or rollback/signature qualification was
run for this slice, following the requested focused live loop.

## First live slice — B1–B3, 2026-11-11

Code commit `ab8a3216f620caf203a36de45f9544097a8285ee` is pushed on
`feat/oib-designer-interface`. Existing `watch-dashboard.py --once` compiled the
Mac tooling, optimized Wasm browser host, ARM direct host/authority and probes,
then activated release
`eb06630bf0c0570fba4a6a3f35afcf19140b156b032bf5fcd2472891b7e19afa`.
Both Frame authority and native supervisor reported that release active. The
existing automatic builder was paused during the build, advanced to this completed
code commit and restored. No additional verification campaign, rollback staging,
state inventory or data preservation was performed.

Focused live acceptance on `https://otrios.otri.ca`:

- A browser already mounted in OIB Designer upgraded automatically. Its WebSocket
  briefly disconnected during authority restart, then recovered to Live shared space.
- Two browser sessions used actual 1280×800 and 1920×1080 CSS viewports on the new
  release. Library/Layers/Inspect retained the canvas; layer selection opened the
  real inspector and its color/text fields without stale hidden-pane geometry.
- In the first session, Test changed Volume from 50.7% to 90%. The second session
  remained at 50.7%, with Brightness unchanged at 56.7% in both. Returning to Edit
  restored 50.7% in the first session. No authored defaults were changed.
- Light editor chrome was inspected visually; authored dark panels and green paint
  remained unchanged. This is editor appearance, not authored component mode support.
- A real inspector field received focus, then Library navigation cleared focus and
  retained the shared values. Prototype and Rust keyboard/capture tests provide the
  additional bounded software-keyboard coverage; physical evdev/touch/color acceptance
  was not performed in this slice.

B1–B3 are live. The overall implementation goal remains active. Next is B4, the
canonical reusable layout model and actual measure/allocation/compiler/inspector
integration, followed by coherent Apply/lifecycle, named styles and catalog journeys.
Do not treat the current imported `Accent` field, per-field Apply or fixed design
frames as delivery of those later slices.

B4 restart map: canonical source `otrios-interfaces/src/oib/graph.rs::Visual` and
`designer.rs::DefinitionNode`; admission/expansion/compiler in core
`canvas/oib/graph.rs`; source codec and canonical round-trip in
`canvas/oib/document.rs`; structural property/admission paths in
`canvas/oib/document/authoring.rs`, `canvas/state/oib.rs` and the Designer helpers.
Shared renderer layout records need actual retained layout support, not a second
editor-only solver. Inspect `dashboard/layout/composition.rs` and `resizing.rs` for
checked arithmetic patterns. Change state protocol v31 in both session.rs and
web/shared/session.mjs when source wire shape changes, preserving control v1.

## Initial B4 implementation — 2026-11-11

Canonical `Visual.layout` lowers to renderer `TemplateNode.layout`, admitted only
in layout v6 with package/library feature `otrios-auto-layout-v1`. Source wire
changes use state protocol v32 on both hosts, with opening control v1 unchanged.
The first implementation adds independent Fixed/Hug/Fill W/H, min/max, horizontal
and vertical flow, nonnegative gap, linked/per-edge padding, physical nine-point
alignment, packing/SpaceBetween, Free coordinates and existing Canvas clipping.
Fixed lengths and Free coordinates use frame expressions; policies do not create
a second copy of those values. Imported reference roots measure intrinsic size
then receive the caller's allocation, so existing pinned library interiors follow
actual width without editing the imported source.

The solver checks bounds/dependency paths, measures bottom-up and allocates
children before resolving descendants. Fill redistribution handles simultaneous
minimum and maximum constraints correctly; overflow retains minima and clipping.
Free children do not contribute to Hug even without container flow. Logical
coordinates retain fractional f32 values. Text Hug uses actual admitted metrics;
the authority caches the verified current reference face once, and native/browser
hosts install their admitted font before layout. Font replacement invalidates
layout caches. No extra dependency or unsafe code was added.

The real inspector uses existing typed drafts and generation-guarded SetVisual.
Only Fixed axes expose editable fixed lengths. Layout fields fit existing draft,
scene and action budgets; hidden numeric fields are retired when clean. Alignment
is beside Gap, Padding and Distribution. A blank component is Vertical, Fixed W
320, Hug H with minimum 48, padding 16, gap 8. These B4 edits still use the existing
per-field/operation commit boundary; B5 must replace that with one working copy.

Local evidence before live publication:

- Designer suite: **62 passed**, one existing timing diagnostic ignored; three new
  cases traverse real authority receipts and inspect resolved shared geometry.
- Auto Layout filter: **24 passed**, one timing diagnostic ignored. Includes 54
  direction/alignment/sizing combinations, mixed min/max redistribution, cycles,
  Free, fractional geometry, clipping/hits, exact font measurement, independent H,
  cache classification and signed-zero encoding, bundle/library capability gates.
- Shared layout regression: **42 passed**, one timing diagnostic ignored before
  the final Free-without-flow case; final rerun remains part of integration.
- Actual immutable imported Slider in local LabelledSlider wrapper: passes at
  design widths 320 and 520, with allocated reference/root/track widths 288 and 488.
- Optimized Mac diagnostic: 500 full layout+Scene resolutions × 202 nodes = 101,000
  nodes in 23.076 ms, about 4,376,786 nodes/sec. This excludes GL, physical input and
  Frame timing; it is not a 100K-scene admission claim. Scene/control limits remain.
- Dashboard all-targets compile check and strict core library Clippy passed.
  Browser session/control tests passed 15/15, including incompatible-state upgrade
  discovery and native/browser codec agreement. No broad repository preflight.

Explicit remaining work: multiline/wrapping Hug needs a shared wrapping painter;
zero-area unclipped containers with visible children remain rejected until
zero-extent scene support is qualified. Minimum overflow geometry exists, but
separate overflow/all-Free-Hug diagnostic presentation remains open. Definition
editing still needs B5's dedicated working-copy preview: unused definitions are
not shown in the app canvas until inserted. These are tracked requirements, not
claims of completed final design. Named styles/modes and Q01–Q16 journeys remain.

### B4 live activation and focused polish

Initial B4 code `1f3e4b0cfe0c4f3f8d12be341e07f588f6969502` activated release
`868a9d712627fc182275ac9266e73b4cf0ce1e766e9f9887b9204a6050726b96` on Frame/Web.
The already-connected v31 browser upgraded to the v32 release and returned to
Live shared space. Native ready reported this same runtime/UI release, OIB Designer,
stream ready, confirmed state and no input capture. Automatic builder restored on
this completed code pin.

Through the live browser, created a blank definition and opened its root by
clicking the visible 24-unit portion of a clipped hierarchy row. Fixed W320 and
Hug H appeared independently. Selecting Fixed H exposed H48 without changing W;
returning to Hug removed that ineffective numeric field. One instrumented sizing
edit took 237.9 ms from pointerdown to the confirmed canvas accessibility summary
(includes Frame/network/browser work, not photon timing; one sample). Scrolling
revealed the actual compact alignment, Gap, Padding and Distribution controls.

Visual inspection caught undersized captions, a long padding-toggle label and an
unadmitted checkmark glyph. The focused correction uses 16-unit capital-height
captions, a shorter toggle label and a drawn input-pass-through checkbox with real
checked semantics. Focused layout editor tests pass4/4; full Designer tests pass
63 with one existing timing diagnostic ignored. Added read-only device-local
workspace/task/mode/selection/authoring diagnostics to distinguish actual native
editor selection during input acceptance. This is diagnostic metadata, not a new
state protocol or product control. Final polish deployment/acceptance follows.

A first synthetic evdev navigation check captured/released both touches, but its
inspector scroll-limit assertion failed; it did not prove the intended definition
selection. Do not record that as native authoring acceptance. Use the new workspace
report to resolve the exact pane/selection/role before further targeted taps.
