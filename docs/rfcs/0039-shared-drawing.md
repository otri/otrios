<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0039: bounded shared drawing and local camera

Status: implemented; live acceptance recorded in
[shared Touch Tracing](../experiments/shared-touch-tracing-2026-11-23.md).

Touch Tracing opts into a reusable drawing surface using `InkOptions.drawing`.
There is no app-name switch. Legacy local retained ink and shared fading ink keep
their contracts. The drawing is app data inside its Space-owned Merkle document;
no second service, socket, file database or camera synchronization is introduced.

## Data and publication

The optional Canvas subtree 5 contains a drawing head (Clear epoch and bounded
ring cursor) and at most 64 segment leaves. Each segment has a random 128-bit
identity, its Clear epoch, authority-bound writer identity and at most 192 points.
Coordinates are signed quarter-pixel world units, bounded to ±65,536 logical
units. Long strokes share segment endpoints. Each update changes the active
segment leaf and the normal durable writer receipt; unrelated segments retain
identical hashes. Normal app checkpoints, journals, admission and publication
apply. Portable drawing export is explicitly unsupported rather than dropping it.

`StateRequest::Drawing` carries a complete bounded segment prefix or Clear. The
authority merges distinct identities, accepts only monotonic prefixes from the
owning writer, and acknowledges exact request replays through existing receipts.
Clear advances the drawing epoch; late writes from an earlier epoch are harmless
no-ops with acknowledged receipts. The retained ring evicts oldest segments at
capacity. Clients coalesce only unissued prefixes of the same segment, keeping
issued commands intact. A read-only lease cannot write or Clear.

Subscriptions explicitly cover subtree 5. Drawing support enables ordinary app
Edit admission even with an empty legacy scalar schema. Existing empty Touch
Tracing datasets can acquire this optional subtree without changing their scope
or scalar-schema identity. An explicit operator policy refresh reuses the Space's
existing audience, memberships and collection IDs to enable the new app action.
State protocol advances to v44; release-control v1 remains unchanged.

## View and inputs

Every device has an independent uniform camera over shared world coordinates.
The empty reference view is 1280×800; the grid fills the viewport with square
world cells. At distant zoom levels the grid uses coarser multiples of its base
spacing to retain readable lines and bounded geometry. Camera motion changes no
app data. Fit Drawing fits retained stroke bounds, or restores the empty reference.
Viewport changes retain the same camera center and relative magnification.

One touch draws. Two touches pan and pinch around their centroid; the remaining
finger cannot accidentally resume drawing after a two-touch gesture. Mouse left
drag draws; right/middle or Space drag pans. Wheel zoom anchors at the pointer.
Native type-B evdev contacts retain their slot identities through the same pointer
path. Browser trackpad pinch uses Ctrl-wheel or Safari gesture events; direct touch
pointer gestures take priority to avoid applying one pinch twice.

Developer Mode exposes a persisted self-profile switch between wheel-pan and
wheel-zoom. Pinch zoom and mouse pan shortcuts work in either setting. There is
no permanently visible Mouse/Trackpad mode selector. The setting grants no access.

Projected segments are clipped before scene admission. Adaptive grid spacing also
reserves a bounded node budget at maximum viewport sizes. Drawing subtree hash
changes invalidate cached presentation independently of scalar-field selectors.
