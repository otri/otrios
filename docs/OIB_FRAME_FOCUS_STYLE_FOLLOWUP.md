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
