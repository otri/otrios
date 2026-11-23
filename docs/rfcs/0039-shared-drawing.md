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
Initial drawing support used state protocol v44. Viewport presence advances the
shared state/auxiliary negotiation to v45; release-control v1 remains unchanged.

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

## Retained GPU path and viewport presence — 2026-11-23

Completed geometry stays in immutable chunk-local endpoint-pair VBOs. Chunks
contain at most 64 points (63 segments); adjacent chunks share an endpoint.
Appending a point rebuilds only the changing tail chunk. Camera changes reuse
resource identities and change scale/origin uniforms. Chunk bounds reject
invisible draws before transfer, upload and GPU submission, while hidden retained
buffers remain available for a later pan. Clear and ring eviction retire their
resource identities. At most 256 chunks are retained for the 64 × 192 point
app-data bound. A full chunk has 2,268 float components (9,072 bytes).

The common GLES2/WebGL1 shader expands endpoint pairs into anti-aliased line
quads; the grid is a separate procedural pass. It uses bounded camera-relative
coordinates and adaptive square spacing. Base/background, world drawing and UI
overlay remain ordered layers. The native safe FFI and Web worker bridge copy
new geometry only when an immutable resource version first becomes visible;
context replay resends visible resources. Presentation statistics expose rebuilt,
visible and retained chunks/bytes.

The baseline uses float endpoints. Frame reports Mali400 with zero vertex-texture
units, ruling out a required texture-indexed vertex path. Its Cortex-A7 exposes
NEON/VFPv4, but retention avoids the repeated CPU projection/tessellation entirely.
RFC 0019's earlier fixed/SIMD experiments did not justify replacing the common
float path. Short coordinate packing remains a separate measured optimization,
not part of this implementation.

Each drawing subscription advertises its visible world rectangle through the
existing scoped auxiliary channel, not the Merkle document. `OTVIEW1` packets
carry four signed quarter-unit bounds and an activity flag; the authority stamps
the peer ID and current OTriOS profile display name (Visitor for guests). Brass
Relay calling cards remain room-specific. Device sessions have distinct peer IDs
and short suffixes even when they share an account. Client-supplied names and
peer IDs cannot impersonate another participant.

Presence is sent at most 10 Hz, with 1.5-second stationary keepalives. The relay
compacts to the latest viewport per peer and expires it after five seconds. Normal
Space/app subscription permission checks and revocation apply; read-only viewers
can share presence without acquiring Edit permission. No extra socket or service
is created. Reconnect clears stale local peers and receives a fresh scoped hello.

While the local user pans, zooms or fits, other peer rectangles and labels appear
with consistent per-session colours, then fade after one second and disappear at
1.5 seconds. Up to 64 peers are admitted. Outlines are clipped before bounded scene
admission and use explicit input pass-through. The local camera remains local;
neither presence nor camera motion writes app history.
