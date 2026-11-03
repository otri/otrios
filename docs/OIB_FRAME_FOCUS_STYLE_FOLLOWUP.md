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
