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

## Initial inventory findings that constrain implementation

These describe the starting implementation; later implementation and acceptance
records supersede them.

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

Parent owns technical plan, integration, cross-contract decisions and final
validation. Runtime/history and UI/dialog roles work in explicitly assigned
disjoint files; read-only information gathering uses Terra. Initial geometry and
sample-adapter roles are complete, not presumed active after resuming context.
Builds sharing the target cache run sequentially. Read known restart paths below
and compiler-reported files rather than performing folder inventories.

Last verified live: code `bdc035c`, release
`9df7bc6ca06bbc2b1013809cc6ad0734ab34f09b89bb096578119dd633047298`,
including named literal endpoints, definition deletion/restoration and the earlier
B5 slices. Focused Frame/Web evidence and remaining UX findings are recorded at
the end of this file. Full B4–B7 gates remain open.
Retired the host profile selector, divider
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
checked arithmetic patterns. B4 changed state v31 to v32 in session.rs and
web/shared/session.mjs. Future source wire changes must bump both again, preserving
control v1.

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
  the final Free-without-flow case; the subsequent Auto Layout filter passed 24
  cases including that final regression.
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
checked semantics. Focused layout editor tests pass 4/4; full Designer tests pass
63 with one existing timing diagnostic ignored. Added read-only device-local
workspace/task/mode/selection/authoring diagnostics to distinguish actual native
editor selection during input acceptance. This is diagnostic metadata, not a new
state protocol or product control. Final polish deployment/acceptance follows.

A first synthetic evdev navigation check captured/released both touches, but its
inspector scroll-limit assertion failed; it did not prove the intended definition
selection. Do not record that as native authoring acceptance. Use the new workspace
report to resolve the exact pane/selection/role before further targeted taps.


### Final B4 polish acceptance

Polish code `75f1b95b622e1bccd3af509a9eea3466058d3a7b` activated release
`c876fca2ce57e59287b6457a41bd36763b7f5dd1ceb93799536fc16cdd4cd5a2` on both hosts.
The builder resumed on that completed code pin. Existing browser source edits
confirmed after the update: Padding 40, independent Fixed/Hug H, and insertion of
the new preset. Volume 50.7% and Brightness 56.7% stayed unchanged. Added a real
96×48 shape to the new component through Library; the live scrolled preview
rendered it at x56/y464, matching outer x16 plus padding 40 and the confirmed
preview offset 480. The source's Hug extent grows with that admitted child.
Caption/readability and drawn-checkbox fixes were inspected visually on the live
browser; the checkbox has no fallback glyph or overlapping padding label.

The first native selection assertion was a harness error: the native client had
that definition collapsed, while the creating browser had it expanded. The new
read-only diagnostics identified the actual pane and selection. Synthetic evdev
contacts through the existing calibrated reader then proved:

- Library selection opens the intended definition 63 in Inspect, authoring enabled,
  with inspector scroll limit 1504; each pointer captures and releases normally.
- Native Height Fixed commits generation 28; the separate browser receives H48
  alongside W320. Native Height Hug commits generation 29; the browser removes the
  fixed H input, retaining W320 and the business values.
- After the browser adds the child (generation 30), native preview scrolling reaches
  offset 480. A native tap selects the visible shape at y464 as instance 66/node 67,
  with the actual Inspect selection reported. The Frame remains on this live view.

These are software-injected contacts through the real native input/renderer path,
not human touch/color acceptance. Calibration files were unchanged. The failed
initial selection check is resolved; no product input bug or role change was needed.
Final targeted tests: layout editor 4/4; Designer 63 passed plus one pre-existing
ignored timing diagnostic; strict core Clippy passed. No further folder inventory
or broad source scan is needed; the user explicitly requested targeted files only.

## Historical B5 planning record

The atomic boundary and private preview below have since been implemented. Use the final B5 acceptance/restart section for current work.

Use already-identified files; do not repeat repository-wide inventories.
`canvas/state/oib/structure.rs::{oib_validate_structure,oib_structure_write}` stages
an expected-structural-generation candidate and compiles before swapping once.
`canvas/state/oib.rs::{oib_write,activate_oib}` owns causal property writes and the
separate UI generation. Public `oib::Change` contains semantic address, value and
observed leaf Snapshot. `canvas.rs::StateRequest::{OibInstance,OibWrite}` and
state.rs receive/Flight/receipts currently send these independently.

T06 requires one bounded OIB-only Apply with base epoch, structural generation,
property contexts and exact import closure. Refactor shared staging on one private
candidate; validate every context before publication, activate once, reuse the
existing sequence/digest receipt/replay boundary. Do not compose two publications
or introduce general transactions. Explicit rejection/result after a lost reply
must survive later status changes; current global status.error is insufficient.
New local identities stay preview-only until Apply; expected-generation admission
must prevent collisions while assigning their final identities coherently.

Preview needs the same compiler over a private source/preset/occurrence for the
selected definition, including unused definitions, without persistent app Insert.
Test's transportless `preview_clone` is only a sample overlay, not this structural
working copy. Reuse `graph::{with_source_prepared,expand_prepared}` with bounded
preview-only identity/slots and last-valid scene feedback. Keep dirty source,
field drafts and source conflict review explicit; no Apply-on-blur in the final UX.

Live findings to resolve in that lifecycle slice: New component should immediately
open the blank editor; Library's Link tools currently list local definitions but
need admitted imported exports for the real LabelledSlider journey; deleting a
component in use is filtered out today and needs visible impact/reason feedback.
Full row qualification remains pending alongside named styles/modes, independent
scroll owners and the previously listed overflow/wrapping/zero-extent gates.

## B5 integration checkpoint — 2026-11-11

Status: **IN PROGRESS; not deployed**. B1–B4 live evidence above is historical;
it does not establish acceptance of this B5 work. The current editor uses the
sole live adapter for confirmed state and a bounded local working copy. One OIB
Apply atomically stages source and causal changes on a private candidate, with a
maximum of 16 operations and 8 KiB, then validates and publishes once. State
protocol is v33 on native/server and browser; the opening release-control envelope
remains v1. Local undo history is bounded to 64 KiB.

The working copy pins the epoch, confirmed baseline, structural generation and
exact causal contexts. A durable per-writer digest receipt supports reconciliation
of the submitted operation; new targets receive a seeded context. Preview compiles
the private candidate, including a private virtual instance of the selected
definition with sample values. Semantic values are projected into that preview
without changing imported bodies. Edit/Test mode, local drafts and canonical
source generation have distinct roles: a local draft or mode change is not a
confirmed canonical generation.

Focused evidence so far: Apply **11**, working choice **1**, and preview/clip
**12** tests passed; workspace **6**, focus **4**, and layout **4** tests passed.
The integrated Designer suite passed **66 tests**, with one existing ignored timing diagnostic. A focused real-process WebSocket test passed against a freshly built macOS canvas-server, covering authorization, mixed publication, lost reply, authority restart and idempotent receipt recovery; this is not Linux IPC acceptance.
These are local tests, not live Frame/Web qualification.

Remaining B5 work includes conditional post-Apply Undo, typed API and slots UI,
retirement and dependency lifecycle, and full impact/replacement review. Apply/Discard/Stay leave confirmation is implemented and passed three focused tests for exact-receipt navigation, rejection, concurrent edits, raw fields and input barriers. Full B6 styles, catalog qualification and live acceptance remain.
Do not mark the goal complete from this checkpoint.

Restart map: inspect `canvas/state/oib/apply.rs` for atomic staging and receipts;
`client/oib_working_copy.rs` for pinned context and local history;
`client/oib_editor.rs` for editor lifecycle; and `oib/graph_preview.rs` for
private preview and semantic projection. The guest activation/reconnect resource regression is fixed: matching qualified compositions reuse their exact validated guest set and resumed locals, without a third allocation or increased limits (14 component tests passed). The final added private-source Test regression also passed. For publication, use the existing publisher and perform focused live Frame/Web acceptance. The remaining B5 capabilities stay open after this slice. Complete B6 styles and
catalog/live qualification separately before claiming the full goal.

### B5 validation before first publication

- `python3 /tmp/otrios-d2-cargo.py test -p otrios-core --features canvas --lib oib_ --locked --offline`: 79 passed, one existing ignored diagnostic, plus the subsequently added `working_copy_composition_test` regression passed separately.
- Same wrapper/library command with `atomic_apply`: 11 passed; `graph::preview::tests`: 12 passed; `component_behavior_tests`: 14 passed after fixing redundant guest allocation; `oib_leave`: three passed in the focused role run.
- `node --test scripts/test-dashboard-session.mjs scripts/test-ui-update.mjs`: 22 passed; native/server and browser declare state v33, control v1.
- Fresh `canvas-server` build and `OTRIOS_SPACES_SERVER=<mac-target/debug/canvas-server>` integration test `d1::b5_atomic_apply_mixed_publication_lost_reply_restart_replay_and_admin_gate`: one passed over real macOS authority-process/WebSocket boundaries; not Linux IPC qualification.
- Core library Clippy with `-D warnings` passed before final source formatting. The final run is recorded with publication evidence below.

The original repository remains on main; implementation stays on the feature branch. No full preflight, data preservation inventory, separate signing campaign or new PR was run. Initial B5 publication is a coherent slice; full typed API/slots, conditional post-Apply history, imported linking, impact/retirement and B6/B7 qualification remain open.

### Initial B5 publication and pointer correction

Commit `685854f` built and published through the existing Frame/Web path as release
`15b4f1a4a4d2a8e476a0bb9a7acc4ee445d0d94d6fa161dba960d700b48c8adb`.
The attached browser upgraded automatically; native ready reported that exact
release, stream ready, confirmed state, authoring enabled and zero guest errors.
The automatic builder was restored on the completed source commit.

The first physical browser click on New component exposed missing outer-toolbar
pointer capture. Semantic activation tests had not exercised that route. The
correction shares one explicit outside-panel action classification between Down
capture and matching Up hit validation, including all five lifecycle commands.
A real Client pointer regression now covers New/Undo/Redo/Discard/Apply,
release-outside and Cancel, proving no publication until Apply. Final Designer
suite: **68 passed**, one existing ignored diagnostic; strict core library Clippy
passed. The corrected release still requires live pointer acceptance below.

## Corrected B5 live acceptance — 2026-11-11

Code commit `a5b1e3a9a68f30f5b727821ec584da347719a230` is published as
`4faf7ae090027833d1a3773236b2a790928a9ba42adb43a18311f5e40295e3ab`.
Both Frame and attached browser activated that exact release. The automatic
builder was restored and its source pin advanced to the completed code commit.
Protocol: state v33/control v1. No PR or broad preflight was opened/run.

Actual focused browser/native behavior:

- At 1280×800, actual pointer clicks New → Undo → Redo → Discard changed the local
  copy without changing the authority's generation 30. New immediately opened an
  unused definition preview; no app Insert was needed. Redo restored the source,
  but after undoing creation the selection remained Document: history selection
  restoration is still a follow-up, not completed UX.
- A calibrated synthetic native evdev New tap selected private definition 68,
  reported reload_ready=false, zero guest instances/errors in its blank preview,
  and retained shared generation 30 and the same three app instances. Native
  Discard returned to Document, reload_ready=true and unchanged shared state.
- The browser created definition 68, set W456 and renamed it `B5 live component`.
  Enter staged each field locally. One global Apply confirmed generation 31;
  the Frame received it without a new app instance. Native Library scrolling and
  selecting the shared component opened Definition(68) in Inspect.
- Native Height Fixed then global Apply produced generation 32. The independent
  browser received H48 alongside W456, with All changes applied. Frame ready
  remained stream_ready/state_confirmed/reload_ready=true, input_busy=false and
  zero guest errors. The Frame is left on this component in Inspect.
- Browser W500 remained private. Back opened Apply/Discard/Stay; an underlying
  Undo tap was blocked and inputs were removed. Stay retained W500. Back then
  Discard navigated to Home without publication; the Frame stayed at generation
  32. Fresh browser navigation showed original app Volume51% and Brightness57%
  with All changes applied. The owned browser tab was closed.

Native contacts were software-injected through the real accepted calibration,
ABS_X/ABS_Y reader, shared input router and GLES host. This is not human physical
touch/color qualification. The first harness attempt used MT coordinates, which
this reader ignores; it was corrected to the actual reader's ABS coordinates.
No calibration, service identity or role changed. Two immediate ready-probe SSH
reads transiently failed; the later read confirmed the same running process and
generation 32, so no process restart was inferred or performed.

Visual/viewport limits: the preview snapshot tool timed out after definition
editing (`Preview automation snapshot timed out after 15000ms`). DOM observations
and pointer actions stayed responsive. The alternative CUA browser host reported
unavailable. No new visual capture acceptance is claimed. A 1920×1080 CSS viewport
was observed, but its first semantic geometry still showed a uniformly fitted
1280×800 scene (96px controls became129.6px); recheck the settled browser viewport
adapter before claiming final desktop responsive allocation. Returned to actual
1280×800 before the remaining checks.

### Restart here

The working-copy slice is live; the full goal stays **active**. Do not rebuild or
repeat folder inventories simply to recover context. Preserve untracked `work/`.
Known runtime files: `client/oib_working_copy.rs`, `client/oib_editor.rs`,
`client/oib_leave.rs`, `client/oib_designer.rs`, `state/oib/apply.rs`,
`oib/graph_preview.rs`. The native input reader is linux-display `src/touch.c`;
accepted surface mapping is core `dashboard/display.rs`, and native ready reports
come from dashboard `native_worker.rs`. Use the existing private builder config,
not a new service.

Next concrete work:

1. Restore selection coherently through local history; implement conditional
   post-Apply Undo/Redo and before/local/current review, retaining exact contexts.
2. Make Library tools/palette clear and compact: remove duplicate New component
   affordance, admit imported exports for real LabelledSlider linking, and retain
   visible dependency impact/blocked deletion/retirement/replacement journeys.
3. Complete typed API/slots editing and RFC0037 descriptive variables/styles,
   Light/Dark component modes and state recipes; generic legacy Accent remains
   until that real resolver exists. Do not disguise chrome appearance as styling.
4. Recheck actual large-browser viewport allocation and recover visual capture;
   qualify representative controls/flows, wrapping/overflow/scroll/resource gates
   with actual Frame/Web evidence. Prototype or local tests do not close these.

Final focused correctness before the corrected release: Designer68 passed plus
one existing ignored diagnostic; atomic Apply11; dedicated preview/clip12;
component behavior14; leave3; real macOS authority-process/WebSocket test1;
browser session/update22. Strict core library Clippy with `-D warnings` passed.
No full repository preflight or full catalog acceptance is claimed.

## B5 follow-up implementation — published, 2026-11-11

This implementation supersedes `a5b1e3a`/release `4faf7ae0…`; the exact new
publication and acceptance follow below. The following changes are implemented:

- Local command history retains selection and task. Undo creation restores its
  originating selection/task; Redo reopens the created definition. Local and
  published records share one 64 KiB budget, including retained location data.
- One accepted Apply becomes one published Undo unit. Clean Undo/Redo prepares a
  private conditional inverse for explicit Apply. Matching accepted receipts alone
  move stacks; rejection, delayed/coalesced replies and unknown equal-value causal
  writes do not refresh guards. Redo derives from the accepted inverse. Normal
  edits cancel pending inverse classification even if later bytes happen to match.
- Accepted receipts now carry the actual post-publication property-leaf hashes,
  ordered as the request's causal changes. State protocol is v34, control remains
  v1; old stored receipts default to missing proof and cannot establish new undo
  proof. ClearDefault distinguishes an absent authored default from an explicit
  equal value, while preserving the independent causal override register.
- Conditional comparisons show Before/Applied/Current and named component/layer
  context in a modal with fixed actions and bounded scrolling. It gates pointer,
  semantic, text and raw app input. Scroll refreshes only the modal presentation,
  retaining the authored layout cache. Long display summaries explicitly include
  their full character count; full canonical guards remain unchanged. Conflicted
  draft roots refresh a clean working base even if the last valid UI generation
  does not advance, so current alternatives participate in review. An open review
  retains its captured comparisons and pointer owner when newer sources arrive;
  it indicates that Inspect loads the latest values. Closing and retrying Undo
  compares against the latest source rather than silently replacing the dialog.
- Library no longer repeats New component; the global command has its own pure
  builder. Link choices include admitted imported export roots, not private library
  closure helpers. Exact pins and immutable bodies remain unchanged; ownership,
  cycles, grants, final-source checks and scene/control/source budgets still apply.
- A transient OIB batch IdentityPlan reserves explicit IDs across definition and
  preset phases and allocates implicit instances into unreserved holes. Claims
  must finish as one exact contiguous authority-counter interval; duplicates,
  retired IDs and holes reject the complete batch. The plan is cleared before
  publication and is not durable state. This permits New component → Add text →
  Link imported Slider plus a new-node causal edit in one Apply without renumbering
  private addresses. There is no general transaction or source-replacement API.
- A generic signed Blueprint ViewportPolicy defaults to Fit with omitted legacy
  bytes. Layout v7/capability `otrios-responsive-viewport-v1` admits Responsive with
  finite minimum dimensions. Designer opts into 800×480 minimum; the browser uses
  available window size within the existing 8192 coordinate bound. Other apps'
  default Fit and existing Chat/Map behavior remain. Source and compositionless
  definition previews retain policy; no app-ID or Frame/Desktop profile selection.
  Captured resize preserves its Up event and publishes a new viewport revision
  only when scene extent commits. Core reconciles requested versus presented size
  even when the next request repeats the cached dimensions.

Known gates remain: conditional graph/deletion restoration with fresh IDs and
creation-Undo Redo are explicitly unavailable, not retired-ID resurrection; typed
API/slots and dependency impact/retirement/replacement still need their full UI;
styles/modes and representative catalog qualification remain open. Some binding
and property review labels still fall back to numeric identifiers and need signed
human descriptor names with the style/API work. The Panel+Text+imported Slider
fixture proves real composition, not completion of the representative catalog.

Known paths for restarting: `client/oib_applied_history.rs`,
`client/oib_applied_history_tests.rs`, `client/oib_history_review.rs`,
`client/oib_history_review_tests.rs`, `client/oib_viewport_tests.rs`,
`oib/history.rs` and `oib/history/text.rs`; identity staging is in
`state/oib/structure.rs`. Viewport host code is `web/shared/viewport.mjs`,
`web/canvas/app.js`, `web/canvas/state-worker.js`, `crates/otrios-web-host/src/lib.rs`
and public `interfaces/src/layout.rs`. Use compiler-reported files for Blueprint
literal defaults, not folder inventories. No dependencies or ceilings increased.

Focused evidence before publication:

- Designer: 74 passed, one existing ignored diagnostic; actual one-Apply
  LabelledSlider composition and source-preserving selection/task history passed.
- Backend conditional Apply/history/identity: 18 passed; full OIB state module:
  37 passed, one existing ignored diagnostic.
- Published history manager: all nine focused cases passed, including the
  conflicted draft/nonadvancing-generation clean-base regression.
- Review UI: four passed, including 16-row scrolling, long-value summaries,
  input barriers, cached-layout reuse, 512-node/128-control bounds and an incoming
  authority edit while a dialog action owns a held pointer.
- Responsive Rust: 21 passed; JS viewport/worker: 24 passed; v34 session/update:
  22 passed; component guest behavior: 14 passed.
- Fresh v34 macOS canvas-server and the real WebSocket B5 test passed: exact
  property proof persisted across lost reply/restart and ClearDefault retained
  the causal override. One previously started legacy D2 process test also passed.
  These are real authority-process/socket proofs, not Linux IPC acceptance.
- Dashboard artifact compilation with canvas/server/host-sync/direct and
  `--all-targets --locked --offline` passed. Compiler-discovered legacy Blueprint
  fixtures received explicit default Fit; legacy wire bytes remain unchanged.
- Final strict core library Clippy (`--features canvas --lib --locked --offline
  -- -D warnings`) and `git diff --check` passed after the clean-base and held
  review fixes. No full preflight or new PR. Existing live publication
  authorization remains in effect.

### Live acceptance of commit `2f95205`

`python3 /tmp/otrios-designer-live-build.py` completed successfully through the
existing publisher: Mac, Wasm and ARM artifacts built, Frame and native code
activated release
`2fcbc24b1a5dbdd065465bf3a12801f36623a8e9af46a25d9fcaaa3a64de3549`.
The automatic builder was paused during publication, restored afterward, and its
source pin advanced to exact commit `2f95205326cef9097d2f5188a31aa90786cf1fdb`.
No new services, identities, ceilings or dependencies were introduced.

- An already-open browser upgraded without manual reload. At 1920×1080, the
  previous release's 1728×1080 fitted canvas/129.59px Library tab became a full
  1920×1080 canvas/96px tab. A selected-definition preview also resized down to
  800×480 and back to 1920×1080, retaining 96px chrome controls and authored content.
  These are actual browser DOM/semantic measurements, not screenshot judgments.
- Real browser pointers performed New → Undo → Redo. Undo restored the original
  Document inspector; Redo reopened the new definition. Add text and Link
  LabeledSlider then succeeded in one global Apply, observed on the Frame as
  generation 34. The live document exposes its local LabeledSlider; the imported
  export path remains covered by the focused admitted-import integration fixture,
  and is not claimed as a live imported-library acceptance.
- A subsequent width change 320→480 published at generation 35. Clean Undo
  prepared 320 privately while the Frame stayed at generation 35. Apply published
  the inverse, then Redo prepared 480 and Apply published the accepted inverse's
  inverse. The new component remains uninstantiated; app instance IDs 20/30/66
  and sample Volume51%/Brightness57% were unchanged.
- Two independent browser editors exercised real concurrent source publication.
  Editor A applied the name `B5 responsive composition`; editor B applied
  `B5 concurrent rename`. A's Undo displayed Before `New component`, Applied
  `B5 responsive composition`, Current `B5 concurrent rename`, with no shared
  write. Clicking the background New command did not create a private edit and
  no text inputs were exposed. B then applied `B5 reviewed composition` while
  the dialog remained open: A retained captured comparisons and showed
  “Sources changed again; Inspect loads latest values.” Inspect current closed
  review and exposed the latest name with All changes applied.
- Native taps used the existing calibrated evdev input path: New selected private
  definition76, with no authority generation advance; Discard returned to Document
  Inspect. Final native report: same release, generation40, state/stream/reload
  ready, input idle, two qualified app guest instances and zero guest errors.
  Both owned browser tabs were closed after the checks.

Visual capture remains unqualified: two preview snapshots failed with
`Preview automation snapshot timed out after 15000ms`; the alternate computer-use
surface reported `The browser access host is unavailable`. DOM, physical browser
pointer, real native input and authority reports above are the actual evidence.
No screenshot/color judgment or human touch acceptance is inferred. The four
focused review tests cover held-pointer concurrency and large bounded review
scrolling beyond this smaller live conflict example.

### Next implementation order

Keep B5 open for fresh-ID restoration/creation Redo, typed API and slots, and
dependency impact/blocked-delete/retire/replace journeys. B6 begins with the
concrete RFC0037 codec, stable owner/field addressing and bounded resolver before
exposing named variables/styles in the inspector. Its first authoring slice must
edit a real Light/Dark color pair through the private working copy and atomic
Apply, retain literals and exact imported owners, and show affected document uses.
Chrome Dark/Light and the legacy Accent property do not satisfy that requirement.
B7 still requires per-control/per-flow evidence in CONTROL_MATRIX. None of these
remaining gates is closed by the publication above.

## B5 fresh graph history — pending publication, 2026-11-11

The next slice replaces the blanket unsupported reason for deleted definition,
node, slot and preset reconstruction. It captures affected source fragments and
explicit override alternatives before deletion, then prepares a conditional
inverse using the current authority high-water mark. Removed identities receive
new IDs; imported definitions and unaffected identities remain exact. It remaps
ownership, visual order, exposed/forwarded paths, slots, presets, affected
occurrence origins and local selection. Fresh override contexts come from a pure
structural preview; the final structure and overrides still publish in one Apply.
This uses existing wire operations: state v34/control v1 and layout v7 remain.

Focused evidence before publication:

- Actual working-copy creation → Apply → Undo → Apply → Redo → Apply, repeated
  twice, preserves a color override at fresh addresses and selects the restored
  definition. Original addresses remain retired; preparation emits no network write.
- Actual Designer/imported Slider + text composition completes creation Undo/Redo
  while retaining exact imported owners, instance allocations and control bounds.
- Actual Designer deleted-node Undo restores edited authored text and a fresh
  selected node; Redo removes that fresh node. Text is a structural source field
  for this primitive, not an invented causal text descriptor.
- The authority test for creation/duplicate wire replay now also publishes a
  fresh-ID definition/root/preset restoration and verifies repeated identical
  request delivery allocates nothing further.
- Three planner cases cover reference/Forward/exposed/slot/preset remapping,
  captured occurrence origin after replacement and prior-conflict rejection,
  plus ownership order independent of visual sibling order. These are mapping
  proofs; synthetic planner graphs are not claimed as admitted/live fixtures.
- `test -p otrios-core --features canvas --lib history --locked --offline`:
  32 passed, including the OIB history/manager/review cases and other matched
  history regressions. The three actual lifecycle integration tests above also
  passed individually. Final strict Clippy and publication are recorded below.

Still open: complete removed occurrence/owned-slot-content capture and restoration,
app-bound graph reconstruction through signed grants, and complete dependency
impact/retire/replace and typed API/slots UI. Oversized inverses retain an explicit
16-operation/8 KiB reason; prior conflicts are never flattened. Full styles/modes
and representative catalog qualification remain B6/B7 gates. No new dependencies
or limits were added. Known restart files: `oib/history/restore.rs`,
`oib/history/restore_tests.rs`, `client/oib_applied_history.rs`,
`client/oib_applied_history_tests.rs`, `client/oib_designer_tests.rs` and
`state/oib/apply.rs` (authored register capture).

### Publication attempt and final local corrections

Commit `95be467` built Mac, Wasm and ARM artifacts successfully. Its existing
Frame publisher then exited with SSH255 / `client_loop: send disconnect: Broken
pipe`; an independent pinned SSH check also timed out connecting to the Frame.
No FRAME/NATIVE/PUBLISHED success was reported. The wrapper restored the automatic
builder and did not advance its source pin. Do not treat this attempt as live
acceptance or restart a presumed-running build: that process is terminal.

The open browser still referenced release `2fcbc24b…` in its module/resource URLs
before its owned tab was closed. Native activation could not be rechecked during
the connection failure. The last verified native state remains generation40 on
that release; current remote state must be probed after reachability returns.

Further local review added two corrections before retrying publication:

- A rival property-only publication can precede deletion while its structural
  generation stays unchanged. The manager now retains deleted-value history only
  when the matching receipt's accepted UI generation is exactly the captured base
  plus one. A new real-authority regression performs that race and verifies Undo
  gives an explicit unavailable reason and emits no stale restoration.
- Affected occurrence origins now use serializable ID pairs, rather than numeric
  Flexbuffers map keys. The planner regression checks actual history-byte encoding;
  the deleted-text Designer test now instantiates the component first and verifies
  deletion/Undo/Redo preserve the existing allocation IDs. This fixes the concrete
  `KeyMustBeString` failure found by the added encoding check.

After these corrections, the repeated creation/override/selection + race case,
the instantiated deleted-text case, imported Slider composition history and the
occurrence-origin encoding case all passed individually. Final strict core Clippy
and `git diff --check` passed. The earlier 32-test history run predates these two
corrections; it is not represented as a final rerun. Retry the completed corrected
commit through the existing publisher when the Frame is reachable, then perform
native creation Apply/Undo/Redo and attached browser checks. Full B5–B7 remain open.

## B5 deletion feedback — local implementation, pending publication

The next batch adds a persistent Library Delete component command for definition
selection, including in-use and imported definitions. It opens a scoped dependency
review with named groups and navigable uses. Known direct/transitive references,
instances, presets, API/slot/app-property connections, exact imported exports and
fixed ownership are reported. Missing/malformed/stale source marks the scan
incomplete and blocks deletion. Imported bodies remain read-only; this is not a
global consumer inventory or a signed-export removal operation.

Unused definitions require explicit confirmation, which recomputes the report
and the complete private admission before staging definition plus palette-preset
removal. Global Apply publishes; fresh-ID history can undo an accepted deletion.
Deleting a component with unapplied literal values prunes only that definition's
pending causal writes, preserving unrelated edits and the full local Undo command.
This closes the reproduced New → color edit → Delete failure rather than asking
authors to discard the entire working copy.

The review retains captured rows and pointer ownership during incoming edits,
marks stale source, blocks its actions and requires Check again. A dirty editor
on older structural source remains blocked after rescan and retains its edits.
Background
semantic/pointer/text/app routes remain gated. Both review types now share one
dialog drag/cancel/release implementation. Dependency buttons support wrapping,
visible clipped hits and offscreen culling. Fitting lists scroll; only display
projections exceeding the existing 8192-coordinate bound at minimum
viewport use bounded pages with no dropped rows. Semantic labels stay within
96 bytes, while visible wrapped text remains complete.

Focused local evidence:

- Dependency query: six passed (direct/transitive/named uses, unused palette
  removals, fixed shell, stale/incomplete source, exact imported owner/closure and
  app-property navigation).
- Deletion UI: five passed, including actual pointer confirmation/navigation,
  no writes before Apply, published Undo, in-use disabled confirmation, stale
  held-pointer capture, clean/dirty rescan behavior, private-value pruning/local Undo and unrelated-edit
  preservation. Actual shared scenes at 800×480, 1280×800 and 1920×1080 stayed
  within 512 nodes/128 controls and retained the authored layout Arc while panning.
- Shared dialog: three passed for wrapped/clipped navigation, culling,
  drag/cancel/foreign release/disabled actions and the unchanged content gate.
  The 300-row paging fixture duplicates only the display projection; it does not
  claim 300 admitted instances. Every index is preserved and fitting reports
  return to a single scrolling page.
- Existing history review: four passed after migrating to shared capture;
  workspace: six passed; leave confirmation: three passed.
- `test -p otrios-core --features canvas --lib canvas::client::oib_designer::tests
  --locked --offline`: 59 passed, one existing ignored diagnostic. This precedes
  the final private-value pruning change; its new deletion tests passed afterward.
  Final strict core library Clippy (`--features canvas --lib --locked --offline
  -- -D warnings`) and `git diff --check` passed after the final changes. No full
  repository preflight or Linux process acceptance is claimed by these local UI tests.

Three pinned Frame SSH probes during this work timed out. No new publication was
started and no live acceptance is claimed for this batch. The previous publisher
is already terminal; the automatic builder remains on the last successful code
pin recorded above. Recheck reachability, publish the completed commit once, then
qualify native creation/deletion/history and attached browser dependency review.

Remaining B5 gates include removed occurrence/owned-slot restoration, typed API
and slots, retirement/unretirement and explicit replacement mappings. Full B6
styles/modes and B7 catalog qualification remain open. Node removal dependencies
still need the same source-aware journey; this component-level review does not
claim that broader UI. Existing admission and limits are preserved; no wire
version, capability, service or dependency was added.

Restart paths: `canvas/oib/impact.rs`, `impact_tests.rs`,
`client/oib_delete.rs`, `oib_delete_tests.rs`, `oib_dialog.rs`,
`oib_dialog_tests.rs`, `oib_workspace.rs`; the Library affordance is in
`client/oib_designer.rs` and scoped pending-value pruning in `oib_working_copy.rs`.

## B5 removed instance history — local implementation, pending publication

The blanket unsupported reason for occurrence removal is replaced by a bounded
captured closure: removed records, placement/frame/owner, connections, sibling
groups, semantic origins and explicit authored values. Restore uses existing
Insert/Move/Connect operations with fresh implicit identities after graph IDs.
Parent-first allocation does not assume old IDs or source order encode ownership.
Slot owners, paths, sibling anchors, selection and connections are remapped.
Existing preset/grant/source/admission and history/operation budgets remain.

Focused evidence:

- A real same-process authority/session test adds an admitted empty slot host,
  inserts two Panel children under Audio, edits parent/child labels and connects
  a child slider to the granted brightness field. One removal Apply removes both
  children and the parent. Private Undo reconstructs fresh IDs, exact prototype,
  child frames/order, owner, connection, authored values and mapped child selection.
  A later app-field update and surviving Display label remain current through
  restoration Apply and Redo. Old addresses remain retired and no preparation
  emits transport writes. This is not Linux IPC acceptance.
- A second authority test inserts a later sibling after deletion. Undo reports
  the changed sibling group including the named later use and publishes nothing.
- A third authority test performs equal-value ABA before deletion/recreation.
  The older command is not reanchored from value equality and the restored value
  remains unchanged. Exact cross-identity causal reanchoring is still a gate.
- The actual Designer management Delete control is exercised with pointer Down/Up,
  then Apply/Undo/Apply/Redo/Apply. Undo selects the fresh root, preserves its
  authored nested label and stays private until Apply; allocation/control bounds
  hold. This qualifies the simple instance path, not an owned-subtree review UI.
- Five planner tests cover parent-first mapping with older children, remapped
  slots/frames/connections/values, combined graph+instance restoration, existing
  conflict/cycle gates and obsolete advisory selection. Mapping fixtures are
  distinguished from the admitted authority fixture above.
- The `history` name filter passed 39 tests, including relevant OIB manager,
  planner and review cases plus other matched history regressions. The final
  additional equal-value ABA test passed separately. Final Designer/lint results
  and publication status follow below.

Final checks: the focused Designer module passed 60 tests with one existing
ignored diagnostic. Strict core library Clippy (`--features canvas --lib --locked
--offline -- -D warnings`) and `git diff --check` passed. No full repository
preflight, cross-platform artifact publication or Linux IPC acceptance was run.
Both pinned Frame SSH probes during this slice timed out. No new publisher was
started, and there is no active publication process to resume. Recheck the
current remote release after reachability returns, publish the completed branch
commit, then qualify the combined lifecycle changes on native and Web.

Remaining B5 work: owned-subtree impact/confirmation, exact causal reanchoring of
earlier commands across recreated IDs, mixed replacement staging capacity, typed
API/slots, app-bound graph reconstruction grants and replacement/retirement UI.
No unavailable capability is presented as successful. Full B6 styles/modes and
B7 representative controls/flows remain open. No wire versions, dependencies or
resource limits changed.

Restart files: `canvas/oib/history/restore.rs`, `history/occurrence_tests.rs`,
`history.rs`, `client/oib_applied_history_tests.rs`,
`client/oib_designer_tests.rs`; property/receipt capture still uses the existing
manager and state adapter paths recorded above. Frame reachability and live
acceptance must be rechecked before claiming this batch deployed.


## B5 owned-instance removal review — local implementation, pending publication

The management action now says **Remove...**, distinct from Library's component
definition deletion. A typed occurrence report follows allocated slot ownership,
checks exact source/import/path/prototype identity, and lists selected/owned
instances, removed explicit connections, retained parents and reusable definitions.
Fixed signed instances and incomplete evidence block removal. An allocated local
instance of an imported definition remains removable. Children precede owners in
source sibling order; numeric IDs do not define ownership order.

The existing dependency modal now supports both typed targets. Opening/Cancel
preserve authored and private state. Confirmation rechecks live and candidate
roots, rebuilds the report and runs ordinary final admission before staging one
private batch. Only the removed closure's raw drafts and staged occurrence
literals are discarded; unrelated edits and shared definitions remain. Global
Apply still publishes. Local Undo restores the prior candidate; published Undo
uses the existing guarded fresh-ID reconstruction. No wire/dependency/limit change.

Focused evidence (same-process authority, not Linux IPC):

- Seven `instance_impact` tests pass: owned children with older IDs/source ordering,
  retained external parent, reusable definitions, imported bodies, fixed ownership,
  missing prototypes/roots, mismatched candidates, cycles and invalid slot paths.
- Six `canvas::client::oib_delete::tests` pass. The new owned-subtree case checks
  real modal scenes at 800×480, 1280×800 and 1920×1080, Cancel preserving all private
  edits, confirmation pruning only closure values, local Undo/Redo, Apply and
  published Undo/Apply. The stale-review case now runs for both definition and
  instance targets, clean and dirty, including a held pointer during remote edit.
- The final Designer module passes 60 tests with one existing ignored diagnostic,
  including explicit confirmation through the renamed Remove... management action.
- The `history` filter passes 40 tests, covering the relevant OIB planner,
  manager, review and causal-state cases plus incidental matched history tests.
- Strict core library Clippy passes. An attempted `--all-targets` strict run also
  encounters duplicate test-fixture module declarations in the existing fixture
  pattern; that broader run is not passing. One new collapsible-if finding was
  corrected. `git diff --check` passes.

Frame SSH timed out again; no publisher was started and none is being waited on.
These changes and the preceding pending lifecycle commits are **not verified live**.
The last verified release above remains the live evidence baseline. Publish and
qualify the combined slice when the existing Frame connection returns. No full
preflight or cross-platform release qualification was run for this slice.

Remaining: exact causal reanchoring of earlier history across recreated IDs,
mixed replacement staging capacity, typed API/slot authoring, app-bound graph
reconstruction and replacement/retirement; B6 styles/modes and B7 controls/flows.
Restart in `canvas/oib/instance_impact.rs`, `client/oib_delete.rs`,
`client/oib_designer.rs`, `client/oib_working_copy.rs` and their focused tests.

## B5 existing typed API routes — local implementation, pending publication

Inspect now has a Component API disclosure in definition scope. It lists the
selected object's inherited declared properties, or a linked component root's
properties when selecting its reference node. Expanding a child property shows
compatible admitted root sources and explicit connect/disconnect commands. Root
selection lists existing forwarding targets by semantic path. Each command runs
ordinary candidate admission before becoming executable, then stages the existing
Expose operation privately. Other targets remain intact. Imported definitions,
read-only mode and type mismatches grant no extra edit authority. Opening the
section does not emit transport writes. Long target paths wrap within the row;
semantic labels remain within 96 bytes and exact paths remain typed actions.

This is T19 slice 1, **not** definition-owned endpoint creation, complete slots,
actions or full LabelledSlider qualification. The existing public contract has no
operation for creating a property descriptor; T19 records the required next schema,
identity, lookup, protocol and history work instead of implying that UI routing
has already provided named `label`/`value` endpoints. No public wire, dependency or
resource ceiling changed in this slice.

Focused same-process evidence:

- `component_api` tests verify a local definition's route into an exact imported
  Slider, private disconnect and local Undo/Redo, stale retired row rejection,
  one structural Apply, reconnect plus root color in one private candidate,
  live forwarded source value, published Undo and unchanged app values.
- The second API case verifies that a Color root offers no Number source, read-only
  projection has no executable route commands, and inspecting an imported body
  publishes nothing. Real scenes at 800×480, 1280×800 and 1920×1080 remain within
  the combined control limit.
- The API row test verifies that long target paths preserve visible text while
  explicitly abbreviating semantic labels at the existing ceiling.

The focused Designer module passes 62 tests with one existing ignored timing
diagnostic; the long-target API row test passes separately. Strict runtime-library
Clippy and `git diff --check` pass. No full preflight or release artifact build was
run for this slice.
Frame SSH again timed out; no publication process was started. Live evidence
remains the last verified release at the top of this file. The API changes and
preceding pending lifecycle commits still require combined native/Web acceptance.
No broad repository scan, full preflight or Linux IPC claim accompanies this slice.

Restart: `client/oib_designer/api_editor.rs`, its parent refresh/disclosure routes,
`client/oib_designer_tests.rs`, shared `oib/document.rs::descriptors` and T19 above.
Continue definition-owned endpoint contracts and slots, alongside the remaining
history/replacement gates and B6/B7; do not close the full objective on these tests.

## B5 named literal endpoints — local implementation, pending publication

T20 implements the declaration part of T19: definition-owned descriptors and a
monotonic contextual property cursor; canonical SetProperty; combined root lookup
through lowering, preview, causal cells, source trace and Inspector; and a real
Expose as <name> command for admitted literal child endpoints. Root Property name
fields use the standard local text editor and typed structural target. Rename
preserves identity. Descriptor updates have guarded history; new-component graph
history reconstructs declarations at fresh definition/root addresses.

Read-only behavior endpoints do not become literal controls. App binding remains
false for these declarations and a named `value` behavior route still needs its
qualified binding/action contract. The combined class/declaration count stays 64;
source/cell/scene/Apply ceilings remain unchanged. No dependencies were added.

The new canonical payload requires state protocol v35. Native/server, browser,
publisher and directly affected protocol fixtures now agree. Release-control v1
is unchanged. A scoped protocol-consistency session test passes all 15 cases,
including incompatible-state release discovery and rejection before state traffic.
This is not a deployment or Linux IPC claim.

Focused same-process evidence:

- The real Designer creates a blank component, links an exact imported Slider,
  exposes its literal Label as a new `label` endpoint, renames it to `caption`,
  and edits its value privately. The compiled shared preview contains the new
  forwarded label text, not merely a source-model assertion. Apply followed by
  component Undo/Apply/Redo restores a fresh definition with the endpoint name,
  contextual ID, cursor, edge and authored value intact.
- Invalid inherited/future IDs, inherited-name collisions, type/default mismatch,
  numeric range on Color and app-binding escalation reject without authority
  mutation. Existing endpoint type changes reject. A standalone endpoint creation
  on an existing component reports the explicit missing inverse; a subsequent
  rename can be applied and conditionally undone with the same endpoint ID.
- Existing forwarding/imported/read-only tests remain passing. Four focused
  component_api tests pass. Final broader checks are recorded after completion.

A negative test found that the fast materialization path did not execute the
full metadata declaration check. The shared declarations validator is now called
from lowering as well as full admission; the test now rejects the collision.

Still open: endpoint deletion/repair and its standalone creation inverse,
read-only behavior binding/action endpoints, slots, remaining history/replacement
work and B6/B7 qualification. The label test does not qualify a complete
LabelledSlider with externally bound value/changed behavior. Frame/Web publication
and acceptance remain pending; no new publisher was started during this slice.


The workspace all-target compile (`check --workspace --all-targets --features
otrios-core/canvas --locked --offline`) passes after updating compiler-reported
ComponentDefinition constructors. It also exposed a pre-existing style integration
test using the removed `surface` helper; the test now calls surface_with_outline
with its explicit LINE color. Focused runtime checks continue below. The Frame SSH
probe timed out again; the v35 cutover has not been published or live-qualified.


Read-only review found and corrected three additional cases:

- Declared endpoints were missing from library export signatures, package
  validation/order and behavior descriptor lookup. Shared declaration validation
  and semantic property signatures now cover these paths. The new library test
  verifies contract change for added/changed types, body-only name/default updates,
  behavior lookup, rejected sparse IDs/cursors, name collisions and policy changes.
- Sparse declarations could not be replayed by fresh-definition reconstruction.
  They now reject under the current contiguous/cursor gate; removal and holes
  require their own qualified contract extension.
- Definition confirmation could retain an orphaned raw endpoint-name buffer and
  then fail Apply. Cleanup now removes only the deleted definition's buffers.
  A real Designer test keeps both an invalid endpoint name and an unrelated
  invalid Width buffer through Cancel; confirmation removes only the endpoint
  buffer, preserves the Width error, and Apply succeeds after the author explicitly
  reverts that unrelated error. Five focused component_api tests pass.

Strict runtime-library Clippy passes. The optional HTTP host probe command was
not executed as a live test: it requires --url and returned usage; Frame SSH still
times out. No host-probe success, deployment or live v35 acceptance is claimed.


Final local qualification for this slice:

- Workspace all-target compile passes with `--features otrios-core/canvas`.
- Designer module: 65 pass, one existing ignored diagnostic. Core OIB: 53 pass.
  State/OIB: 37 pass, one existing ignored diagnostic. The history name filter
  passed 41 tests (overlaps these suites and includes incidental history cases).
- Library module with signed-bundles enabled: eight pass, including the two new
  declared-endpoint/cursor/namespace cases. Style integration: six pass after
  updating the stale helper and making the malformed-exponent case independent
  of the now-configurable outline color.
- Browser session/protocol checks: 15 pass. UI/library update classification:
  seven pass. Strict runtime-library Clippy with signed-bundles and diff checks
  pass. No new dependency or resource limit was introduced.

The empty declaration case preserves an exhausted inherited property namespace;
it requires no successor ID. The API reports unavailable allocation without
losing existing route inspection. Sequential declared IDs/cursor remain required.

These are local compile/unit/Scene and same-process authority results. There is no
new ARM/Wasm release artifact, Linux IPC acceptance or live v35 result. Existing
Frame/Web publication must combine this cutover with the earlier pending slices
once Frame SSH returns. Keep the full goal active; next implement endpoint
removal/impact and its missing inverse, then binding/actions/slots and B6/B7.

Restart files: `oib/document.rs`, `document/admission.rs`, `document/authoring.rs`,
`graph.rs`, `graph_preview.rs`, `history.rs`, `history/restore.rs`,
`client/oib_designer/api_editor.rs`, `state/oib/inspector.rs`, `library.rs`,
`oib/contract.rs`, the focused tests and T20. Protocol v35 is declared in
interfaces/session.rs, web/shared/session.mjs and the existing publisher.

## Live v35 acceptance after Frame recovery — 2026-11-11

This entry supersedes the pending-publication status above. The existing publisher
completed Mac/Wasm/ARM builds and deployment of `bdc035c`; Frame, native and public
Web report release `9df7bc6ca06bbc2b1013809cc6ad0734ab34f09b89bb096578119dd633047298`.
The existing automatic builder was restored with that source pin. No additional
service, identity or trust configuration was introduced.

Live acceptance used an authenticated browser with the existing Frame identity,
an independent read-only browser, and the native Frame client. Shared-runtime
semantic controls drove browser edits; calibrated evdev taps drove native New
component and Discard. These taps are injected input, not human touch observation.

- Created a blank definition, linked LabeledSlider, exposed its text endpoint,
  renamed it `caption`, renamed the component `Frame caption control`, and set
  `Live caption verified`. The private preview changed before Apply; Apply
  published generation 41 to the observer and Frame.
- Undo/Apply removed the new definition at generation 42. Redo/Apply restored the
  named endpoint, forwarding and value with fresh definition identity 80 at 43.
- Definition deletion displayed its dependency report. Background Undo did not
  escape the modal. At 800×480 all three footer actions remained visible. Cancel
  preserved the definition. Confirm/Apply deleted it at generation 44;
  Undo/Apply restored its caption with fresh definition identity 84 at 45.
- Native New component opened a private definition; Discard returned to Document
  with no shared publication. The native client subsequently reached generation
  45, confirmed/stream/reload ready, no pending commands and zero component errors.
- Both browsers reached generation 45 without reported page/component errors.
  The independent observer remained read-only. Existing Audio/Display sample
  values stayed at Volume 51% and Brightness 57% through the authoring journey.

The resource diagnostic reported active generation 45 and presented generation
44 on all clients after the unused-definition change. Do not describe this as
matching presented generations or full rendered-layout acceptance. No Linux IPC
acceptance or broad repository suite was run for this deployment.

Concrete live UX findings to fix before final qualification:

1. Layers selects the composition hierarchy even while editing a definition;
   selecting the new definition's child required the Library hierarchy.
2. Renaming `label` to `caption` updates the heading but leaves the retained
   input's accessible label `Property name: label` until its editor is rebuilt.
3. A newly linked LabeledSlider is clipped by its default 160×100 reference
   viewport and the blank root's width. Resolve initial reference sizing through
   the canonical layout model; this is not polished insertion acceptance.

Endpoint-removal impact files remain an uncompiled, uncommitted follow-up. The
next lifecycle slice still needs removal/history support, followed by bindings,
actions, slots, styles and representative catalog acceptance.

## Follow-up: definition Layers and initial link sizing — 2026-11-11

Layers now chooses the selected definition's admitted hierarchy when editing a
definition/root/child, filtering out unrelated definitions and app occurrences.
Entering Layers reveals that definition's root and children; subsequent explicit
disclosure still controls collapse. Library retains its document-wide definition
list, and composition selection retains the composition tree. History task
restoration uses the same scope rule. A real semantic-control test creates a
component, adds text, navigates Layers and selects that child, then checks
responsive scene budgets and unchanged authority bytes.

The stale endpoint-name accessible label originated in the browser TextInputs
adapter, which only assigned labels when creating a field. Accepted projections
now refresh labels while preserving the existing DOM field, focus, composition,
raw text and selection. All 18 text-input adapter tests pass, including a retained
field rename during composition.

New linked components use canonical item sizing instead of a fixed 160×100
viewport: Fill width in a definite-width Auto Layout parent, Hug width otherwise,
and Hug height. Imported intrinsic expressions stay in their original namespace.
The existing blank-root policy remains Fixed width 320 and Hug height. The real
verified-export composition test checks 288-wide allocation, 112-high linked
content and a 180-high parent with text/gap/padding, before and after Apply.
The Designer suite passed 84 tests with one existing ignored timing diagnostic
before the additional Hug-parent regression was added. Live acceptance for these
follow-ups is pending publication; the preceding release remains the last live
record until the result below is added.
