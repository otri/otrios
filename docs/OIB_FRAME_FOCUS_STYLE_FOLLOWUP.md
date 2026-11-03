<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame Focus shared-style follow-up

2026-11-04. Active, following user feedback after live visual acceptance became
available. Preserve the approved Frame Focus layout, direct editing and scrolling.

## Concrete styling contract

Use existing `otrios_interfaces::theme` tokens: BG, CARD, TEXT, MUTED, BRASS,
COPPER and LINE. Match authored `surface()` and `button()` controls: a 1.5-unit
LINE outline, CARD fill and rounded corners. Use BG for editor fields and the
workspace background. Headers use HEADER_FONT_SCALE (3), body/control text uses
scale 2. Retain current touch targets and bounded geometry; never shrink text.

Hierarchy items and disclosure arrows remain flat navigation rows, with a coherent
LINE/BRASS selection rather than the current ad hoc blue highlight. Scope tabs,
form actions, Info buttons and keyboard keys use the outlined control treatment.
The same theme applies to the dock and drawer surfaces. Decorative surfaces pass
through input; full-row activation, clipped controls and modal barriers remain.

Component accent values remain authored/editable document data. Do not overwrite
live color overrides to impose an editor theme. The specimen's undersized static
button caption may use the standard body scale, provided the exact Seed contract
and retained state remain compatible. No new structural authoring or wire format.

## Bounded ownership

- Surface helper child: only new LGPL `canvas/client/oib_designer/style.rs` and
  `tests/oib_designer_style.rs`. No shared-file edits, exports, app changes, push,
  PR, deployment, private config, SSH, dependencies, unsafe code or subdelegation.
- Parent: Builder, hierarchy, shell, keyboard and inspector integration; optional
  static caption sizing; resource checks, reviewed integration and signed release.
- Fresh independent review after concrete integration: style consistency plus
  geometry/input/resource invariants. Existing ceilings may not be raised.

Helper API (parent adds the module declaration on integration):

```rust
pub(super) fn surface(
    ids: [u32; 3], width: f32, height: f32,
    fill: Color, radius: f32,
) -> SceneNode;
pub(super) fn resize_surface(node: &mut SceneNode, width: f32, height: f32)
    -> Result<(), String>;
pub(super) fn set_surface_fill(node: &mut SceneNode, fill: Color)
    -> Result<(), String>;
```

`surface` returns a pass-through Canvas at origin with explicit IDs for itself,
outer LINE superellipse and inset fill superellipse. Keep finite positive frames
and clamp radius to geometry. The mutation helpers recognize this generated
surface and a plain Superellipse (for existing flat tree rows), preserve IDs,
paint order and pass-through behavior, and reject unexpected nodes without
partial mutation. Do not resize arbitrary scene trees. Shared tokens are the
only palette source. A fill update changes the inner fill, preserving the stroke.

The focused LGPL integration test may include the file by path, reexporting
`otrios_core::dashboard` at its root if needed before the parent module export
exists. Test real Scene admission, outline/inset sizing after resize, fill changes
preserving stroke and IDs, unsupported-node rejection, decorative pass-through
and parent whole-view hits. Keep implementation within core.

## Acceptance gates

1. Confirm shared tokens/typography and real screenshots, not a new visual style.
2. Run focused Designer/style tests and six-root joint viewport/resource checks.
   Preserve 512 nodes, 128 controls, cache and coordinate ceilings.
3. Confirm any specimen caption change preserves exact Seed bytes and bindings;
   test retained old UI -> new UI publication without resetting a collection.
4. Independent review; signed deployment using the existing publisher and a
   compatible code rollback. Coordinate and restore the existing exact-pin watcher.
5. Inspect the actual Frame and signed-in preview after deployment. Smoke-test
   direct entry/reset and the outlined/flat input boundaries. Measure the changed
   hierarchy/inspector paint path on Frame if added outline runs affect workload;
   do not substitute ready-file reports for actual presentation timing.

## Integration evidence before signed staging

The isolated helper commit `662c2a7` was reviewed and integrated as `525dc95`.
Its five real Scene/input tests passed with both dashboard-only and Canvas
features. Parent integration uses outlined form controls/cards/keys, flat tree
rows, LINE/BRASS selection without asterisks, BG workspace/fields and shared
scale-3 headings. Keyboard paint layers reuse each key canvas to avoid an
unnecessary wrapper per key. Existing key/control IDs and whole-view hits remain.

- 28 Designer tests passed after surface integration; strict core library Clippy
  passed. A subsequent focused six-root keyboard-plus-both-panes regression passed
  in landscape and portrait, with all 38 keys admitted and no OIB writes.
- The new regression sampled joint window edges; maximum scenes were 391 nodes /
  57 controls and 415 / 72, below unchanged 512 / 128 limits. Existing real
  six-root scroll checks also passed; no cache/coordinate limit was raised.
- Nine fixture tests and five surface tests passed.
- A temporary LGPL compatibility probe captured the old UI before the specimen
  caption changed from scale 1 to 2. It confirmed exact Seed bytes, selectors,
  handlers and schema equality, then upgraded a retained document containing
  numeric/text/RGBA overrides and a duplicated panel. Active/draft values,
  allocations, presence and four app values were preserved. The probe passed and
  its temporary source was removed; its old canonical UI remains under ignored
  target/frame-focus for reproduction. No production migration was broadened.

Independent integrated review, signed staging/deployment, final fixture capture
and changed-paint-path Frame timing remain pending for this styling follow-up.
The earlier performance qualification continues to describe its exact prior
release; it is not claimed as measurement of the new outlined paint workload.

## Independent modal-barrier finding and correction

The first integrated review passed 29 Designer, 5 surface and 9 fixture tests,
and 6,381 joint pane-edge cases including provenance details and keyboard off/on.
Maximum scenes were 400 nodes / 58 controls and 419 / 72, below existing limits.
It found one P2 issue: the decorative outlined drawer surface passes through,
while Consume on a Canvas alone cannot establish a leaf hit in a blank gutter.
At [310, 200], the underlying preview slider received an app drag.

The permanent drawer regression reproduced that failure before the fix. The
parent added a transparent, full-area Consume leaf behind drawer controls only
while the drawer is open. Visual layers remain pass-through; foreground controls
retain priority. No handler, semantic control or resource limit was added.
The regression now checks blank left/right/top/bottom gutters in both orientations
through actual InputRouter Down/Move/Up dispatch, as well as the existing closed
layout and release-outside checks. All 29 Designer tests passed after correction.
A fresh independent recheck and rebuilt signed candidate are required before
this follow-up can deploy; the earlier staged artifact is superseded.


## First signed styling measurement and bounded correction

Source `433b876`, signed release
`5e66183daaa83b10673f6e957cf1c8f738f4b152df9b4587be06082d9a0147f7`,
passed signature/closure, authority/native activation and healthy v27 checks.
Actual fixture-only Frame and signed-in browser images confirmed the outlined
controls, dark fields, shared palette and larger type. Browser login survived
the cutover. The four app values stayed [440,700,1000,0].

Actual six-root Frame pans did not pass the unchanged 28 Hz target: hierarchy
24.1935 Hz (p95 53.006 ms), selected inspector 27.6976 Hz (p95 35.623 ms).
Both logs had zero drops, 15 topology rebuilds and unchanged generation 40.
An earlier timing capture was truncated by the controller and discarded; the
valid runs used one sequential controller that restores the host after the full
gesture. Temporary roots 65542–65545 were removed and original width/color
state restored at generation 45. These failures remain recorded, not rounded up.

Read-only investigation found steady rendering cost rather than repeated mesh
uploads. The transparent modal shield introduced for correct blank-gutter hits
forced native blending across its entire retained geometry slot. The bounded
correction uses opaque CARD behind the fully covering square-corner drawer;
identity, geometry, responder, ordering and visible appearance are unchanged.
Independent review found no correctness objections. Its disposable host probe
checked two/six roots, both orientations and keyboard off/on: all eight fixed
overlays had zero nonopaque geometry vertices, versus six with the transparent
shield restored. Hit/capture results matched. The permanent drawer test passed.

Source `e822802` is the single-variable signed candidate. Its actual Frame
performance is still pending; the blending diagnosis alone is not acceptance.

The signed opaque-shield experiment (release `7604ff8a`, source `e822802`)
verified and activated normally. The same actual six-root hierarchy trial gave
24.5271 Hz, p95 53.004 ms and maximum 53.076 ms, zero dropped logs and 15 rebuilds;
generation 49 stayed unchanged. It still fails the 28 Hz gate. Disabling the
shield-triggered blending therefore does not explain most of the regression.
The normal service was restored; temporary roots 65546–65549 were removed.
App values and original overrides remain unchanged. Further work targets repeated
geometry state setup between consecutive outlined fills; hardware acceptance and
release automation promotion remain incomplete.

The slot-local geometry-binding change (`cf4dbcc`, release `c1c14dca`) passed
its actual-source GL harness, sanitizer runs and independent 20,000-case
base/HEAD trace equivalence check. Matching ARM/wasm builds and signed closure
verification passed; authority/native activation and health passed. Actual
hierarchy timing remained below target: 24.7942 Hz, p95 53.006 ms, maximum
53.098 ms, zero drops and 15 topology rebuilds. Generation 57 stayed unchanged
through the gesture. Temporary roots 65550–65553 were removed; the original two
roots, values and overrides were confirmed at generation 61. This call-count
optimization is not a claimed performance qualification. Next work examines
overlapping bounded composition with the display's present wait while keeping
actual presentation acknowledgements and input targets tied to completed flips.


## Overlap review and release authorization correction

The first bounded overlap candidate (`ab87763`, signed `060c83a3`) built and
verified but was never activated. Independent review reproduced a P1: late
Presented could publish old idle upgrade flags while the worker held a new
gesture. The supervisor's exact hot-reload predicate accepted the stale report.

The correction keeps completed-frame identity separate from current safety.
Once a displayed report exists, the worker updates input/confirmation/pending and
reload fields whenever its current state changes, even without renderer credit.
An executable exit request now asks the state owner for quiescent admission. It
denies held gestures and connected pending edits; an accepted grant seals the
old host's input boundary before shutdown. The native input thread continues
until that grant. The existing supervisor recognizes worker-quiescent-v1 and
waits for the admitted upgrade exit instead of force-closing after five seconds.
Explicit service shutdown and older verified hosts retain their previous path.
Disconnected RAM-edit policy remains unchanged. No state wire change was added.

The regression test bootstraps an actually displayed report, starts a contact,
delivers a late old Presented, verifies current busy/non-reloadable status and
denied exit, naturally releases the contact, then verifies the accepted grant
and rejection of later contacts. The full focused direct-host binary passed
26 tests with local socket access. Supervisor ArtifactBudget and DirectHostContract
passed 17 tests, including more than five seconds of simulated guarded waiting
without termination and explicit service shutdown. Focused formatting and
whitespace checks passed. Fresh independent recheck, ARM build and actual Frame
performance remain required before activating the corrected overlap candidate.
