<!-- SPDX-License-Identifier: Apache-2.0 -->
# Five-second ghost trails

Touch Pad drawings are local presentation state plus ephemeral ink on channel 2
of the client's single `/session` WebSocket. They never enter durable documents,
Monitoring payloads or client document storage. Only the final pad position is a
lasting app assignment on release. Arrow-key steps can also draw short segments.

The server assigns an anonymous peer identity; clients choose its presentation
color deterministically. Down/up boundaries keep strokes separate. Trails fade
for five seconds after their latest point. Replayed points carry server age, so
late joins do not make old ink young again. Local presentation uses a monotonic
clock and continues while durable state waits for confirmation.

Both input and painting use one top-left normalized coordinate convention in the
admitted pad's local frame. The retired inset/inverted-Y compatibility mapping is
removed. Parent transforms, clipping and captured gestures use the common scene
and responder implementation on native and browser hosts.

Clients bound strokes, points, outgoing packets and replay history. Unsent moves
coalesce while down/up boundaries remain ordered. Slow receivers reconnect or drop
expired ink instead of growing an unbounded durable backlog. The concrete bounds
are documented in [resource ownership](ARCHITECTURE-RESOURCES.md) and the shared
`ink` contract. The server's ink state is memory-only and disappears on restart.

The old `/demo/ink` endpoint is a retired-route tombstone. New clients subscribe
through the opening/control messages on `/session`, independently of state codec
work, while using the same connection.
